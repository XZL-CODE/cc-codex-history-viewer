// 应用内更新的状态：按设置自动检查，记住检查结果与安装进度，供顶栏提示、更新弹窗和设置页共用。
// 自动检查每个整点（本机时间）一次；启动后与窗口重新获得焦点时，本小时还没查过（失败也算查过）也查一次；
// 上次发现了还没安装的新版本时，启动后不管本小时查没查过都查，让提示一直在。

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, errMessage } from "@/lib/api";
import type { UpdateInfo, UpdateProgress } from "@/lib/types";

const AUTO_CHECK_KEY = "cchv-update-auto-check";
const LAST_CHECK_KEY = "cchv-update-last-check";
const UPDATE_PROGRESS_EVENT = "update-progress";
const HOUR_MS = 60 * 60 * 1000;
/** 启动后稍等再查，不和首次索引构建抢资源 */
const STARTUP_DELAY_MS = 5000;
const HOURLY_SLACK_MS = 1000;
/** 整点定时器之外的兜底：睡眠唤醒后定时器可能错过整点，每 5 分钟看一次本小时查过没有 */
const DUE_POLL_MS = 5 * 60 * 1000;

/** 本应用最新一版的 Release 页面：检查或安装失败时交给浏览器下载 */
export const RELEASES_URL =
  "https://github.com/XZL-CODE/cc-codex-history-viewer/releases/latest";
/** 全部版本的 Release 列表：弹窗没列出的更早版本在这里看 */
export const RELEASES_LIST_URL =
  "https://github.com/XZL-CODE/cc-codex-history-viewer/releases";

export type CheckStatus =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "error";

/** 上次检查的时间（失败也记），以及最近一次成功检查发现的新版本 */
interface LastCheck {
  at: number;
  found: string | null;
}

function readAutoCheck(): boolean {
  try {
    return localStorage.getItem(AUTO_CHECK_KEY) !== "off";
  } catch {
    return true;
  }
}

function readLastCheck(): LastCheck | null {
  try {
    const raw = localStorage.getItem(LAST_CHECK_KEY);
    if (!raw) return null;
    const value = JSON.parse(raw) as Partial<LastCheck>;
    if (typeof value.at !== "number") return null;
    return {
      at: value.at,
      found: typeof value.found === "string" ? value.found : null,
    };
  } catch {
    return null;
  }
}

function persist(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* 忽略持久化失败 */
  }
}

/** 本机时间当前整点的时刻 */
function hourStart(now: number): number {
  const date = new Date(now);
  date.setMinutes(0, 0, 0);
  return date.getTime();
}

/** 该自动检查了：从没查过，或本小时还没查过 */
function checkDue(last: LastCheck | null, now: number): boolean {
  return !last || last.at < hourStart(now);
}

interface Updater {
  autoCheck: boolean;
  setAutoCheck: (on: boolean) => void;
  /** 最近一次检查的状态；自动检查失败也只记在这里，不弹提示 */
  status: CheckStatus;
  checkError: string | null;
  /** 检查到的新版本 */
  available: UpdateInfo | null;
  check: () => Promise<void>;

  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;

  installing: boolean;
  progress: UpdateProgress | null;
  installError: string | null;
  install: () => Promise<void>;
}

const UpdaterContext = createContext<Updater | null>(null);

export function UpdaterProvider({ children }: { children: ReactNode }) {
  const [autoCheck, setAutoCheckState] = useState(readAutoCheck);
  const [status, setStatus] = useState<CheckStatus>("idle");
  const [checkError, setCheckError] = useState<string | null>(null);
  const [available, setAvailable] = useState<UpdateInfo | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const checkingRef = useRef(false);
  const installingRef = useRef(false);

  const check = useCallback(async () => {
    if (checkingRef.current || installingRef.current) return;
    checkingRef.current = true;
    setStatus("checking");
    setCheckError(null);
    try {
      const update = await api.checkForUpdate();
      setAvailable(update);
      setStatus(update ? "available" : "upToDate");
      persist(
        LAST_CHECK_KEY,
        JSON.stringify({
          at: Date.now(),
          found: update?.version ?? null,
        } satisfies LastCheck)
      );
    } catch (error) {
      setStatus("error");
      setCheckError(errMessage(error));
      // 失败也算本小时查过，下个整点再试；保留上次发现的新版本
      persist(
        LAST_CHECK_KEY,
        JSON.stringify({
          at: Date.now(),
          found: readLastCheck()?.found ?? null,
        } satisfies LastCheck)
      );
    } finally {
      checkingRef.current = false;
    }
  }, []);

  // 自动检查：每个整点；启动后、窗口重新获得焦点或重新显示时、兜底轮询时，本小时还没查过才查。
  // 关掉开关会取消还没开始的检查
  useEffect(() => {
    if (!autoCheck) return;
    // 启动时窗口也会获得焦点，等过了启动延迟才响应焦点与定时
    let started = false;
    const checkIfDue = () => {
      if (started && checkDue(readLastCheck(), Date.now())) void check();
    };
    const onVisible = () => {
      if (document.visibilityState === "visible") checkIfDue();
    };
    const startup = window.setTimeout(() => {
      started = true;
      const last = readLastCheck();
      if (checkDue(last, Date.now()) || last?.found) void check();
    }, STARTUP_DELAY_MS);
    // 对准下一个整点（多等 1 秒，避免定时器略早触发时还算在上个小时），触发后再对准下一个
    let hourly = 0;
    const scheduleHourly = () => {
      const now = Date.now();
      hourly = window.setTimeout(() => {
        checkIfDue();
        scheduleHourly();
      }, hourStart(now) + HOUR_MS - now + HOURLY_SLACK_MS);
    };
    scheduleHourly();
    const poll = window.setInterval(checkIfDue, DUE_POLL_MS);
    window.addEventListener("focus", checkIfDue);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.clearTimeout(startup);
      window.clearTimeout(hourly);
      window.clearInterval(poll);
      window.removeEventListener("focus", checkIfDue);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [autoCheck, check]);

  // 下载与安装进度
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, (event) =>
      setProgress(event.payload)
    )
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        /* 非 Tauri 环境（如纯浏览器预览）没有事件通道 */
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const setAutoCheck = useCallback((on: boolean) => {
    setAutoCheckState(on);
    persist(AUTO_CHECK_KEY, on ? "on" : "off");
  }, []);

  const install = useCallback(async () => {
    if (installingRef.current) return;
    installingRef.current = true;
    setInstalling(true);
    setInstallError(null);
    setProgress(null);
    try {
      // 成功时应用会重启（Windows 上由安装程序关掉再打开），这里通常等不到返回
      await api.installUpdate();
    } catch (error) {
      setInstallError(errMessage(error));
      setProgress(null);
      installingRef.current = false;
      setInstalling(false);
    }
  }, []);

  const openDialog = useCallback(() => setDialogOpen(true), []);
  const closeDialog = useCallback(() => {
    if (!installingRef.current) setDialogOpen(false);
  }, []);

  const value = useMemo<Updater>(
    () => ({
      autoCheck,
      setAutoCheck,
      status,
      checkError,
      available,
      check,
      dialogOpen,
      openDialog,
      closeDialog,
      installing,
      progress,
      installError,
      install,
    }),
    [
      autoCheck,
      setAutoCheck,
      status,
      checkError,
      available,
      check,
      dialogOpen,
      openDialog,
      closeDialog,
      installing,
      progress,
      installError,
      install,
    ]
  );

  return (
    <UpdaterContext.Provider value={value}>{children}</UpdaterContext.Provider>
  );
}

export function useUpdater(): Updater {
  const value = useContext(UpdaterContext);
  if (!value) throw new Error("useUpdater 必须在 UpdaterProvider 内使用");
  return value;
}
