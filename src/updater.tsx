// 应用内更新的状态：启动后按设置自动检查，记住检查结果与安装进度，供顶栏提示、更新弹窗和设置页共用。
// 自动检查每天最多一次；上次发现了还没安装的新版本时，每次启动都查，让提示一直在。

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
const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
/** 启动后稍等再查，不和首次索引构建抢资源 */
const STARTUP_DELAY_MS = 5000;

/** 本应用最新一版的 Release 页面：检查或安装失败时交给浏览器下载 */
export const RELEASES_URL =
  "https://github.com/XZL-CODE/cc-codex-history-viewer/releases/latest";

export type CheckStatus =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "error";

/** 上次成功检查的时间，以及那次发现的新版本 */
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

/** 该自动检查了：从没查过、距上次超过一天，或上次发现的新版本还没装 */
function autoCheckDue(last: LastCheck | null, now: number): boolean {
  return !last || now - last.at >= CHECK_INTERVAL_MS || last.found !== null;
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
    } finally {
      checkingRef.current = false;
    }
  }, []);

  // 启动后自动检查；关掉开关会取消还没开始的检查
  useEffect(() => {
    if (!autoCheck || !autoCheckDue(readLastCheck(), Date.now())) return;
    const timer = window.setTimeout(() => void check(), STARTUP_DELAY_MS);
    return () => window.clearTimeout(timer);
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
