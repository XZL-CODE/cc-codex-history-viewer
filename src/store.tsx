// 全局轻量状态：主题、Agent 范围、搜索输入、命令过滤、当前文件夹、设置弹窗、索引刷新与进度。

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
import { useQueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, errMessage } from "@/lib/api";
import type { DictKey } from "@/i18n";
import type { AgentFilter, IndexProgress } from "@/lib/types";
import type { SearchScope } from "@/lib/search";

export type { SearchScope };
export type ThemeMode = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

/** 顶层提示条：文案在渲染时按当前语言翻译 */
export interface Notice {
  kind: "info" | "error";
  key: DictKey;
  params?: Record<string, string | number>;
}

const THEME_KEY = "cchv-theme";
const AGENT_FILTER_KEY = "cchv-agent-filter";
const INCLUDE_COMMANDS_KEY = "cchv-include-commands";
const INDEX_PROGRESS_EVENT = "index-progress";
const INFO_NOTICE_MS = 4000;

interface Store {
  theme: ThemeMode;
  resolvedTheme: ResolvedTheme;
  setTheme: (mode: ThemeMode) => void;

  /** 概览页的数据范围，默认合并查看，本机持久化。 */
  agentFilter: AgentFilter;
  setAgentFilter: (agent: AgentFilter) => void;

  /** 侧边栏文件夹与文件夹详情各自维护独立的数据范围。 */
  sidebarAgentFilter: AgentFilter;
  setSidebarAgentFilter: (agent: AgentFilter) => void;
  projectAgentFilter: AgentFilter;
  setProjectAgentFilter: (agent: AgentFilter) => void;

  /** 搜索框即时输入值；搜索页本身以 URL 为准 */
  query: string;
  setQuery: (q: string) => void;

  /** 下一次搜索的范围：全局 / 当前文件夹 */
  scope: SearchScope;
  setScope: (s: SearchScope) => void;

  /** 是否在结果中包含斜杠命令（/clear 等） */
  includeCommands: boolean;
  setIncludeCommands: (b: boolean) => void;

  /** 当前进入的文件夹（真实路径），用于「当前文件夹」搜索 */
  currentProject: string | null;
  currentProjectName: string | null;
  setCurrentProject: (path: string | null, name?: string | null) => void;

  settingsOpen: boolean;
  openSettings: () => void;
  closeSettings: () => void;

  /** 导入 Claude Code 会话弹窗 */
  importOpen: boolean;
  openImport: () => void;
  closeImport: () => void;

  /** 索引构建进度；null 表示当前没有在构建 */
  indexProgress: IndexProgress | null;
  /** 手动刷新是否进行中（首次懒加载不算） */
  refreshing: boolean;
  /** 刷新索引：默认增量，只重解析变化文件；full=true 忽略缓存全量重建 */
  refreshIndex: (full?: boolean) => Promise<void>;

  notice: Notice | null;
  dismissNotice: () => void;
}

const StoreContext = createContext<Store | null>(null);

function readTheme(): ThemeMode {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
  } catch {
    /* localStorage 不可用时使用默认值 */
  }
  return "system";
}

function readAgentFilter(): AgentFilter {
  try {
    const saved = localStorage.getItem(AGENT_FILTER_KEY);
    if (saved === "claude" || saved === "codex" || saved === "all") return saved;
  } catch {
    /* 无效或不可用时回退 */
  }
  return "all";
}

function persist(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* 忽略持久化失败 */
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const [theme, setThemeState] = useState<ThemeMode>(readTheme);
  const [systemDark, setSystemDark] = useState<boolean>(() => {
    try {
      return window.matchMedia("(prefers-color-scheme: dark)").matches;
    } catch {
      return false;
    }
  });
  const [query, setQueryState] = useState("");
  const [agentFilter, setAgentFilterState] = useState<AgentFilter>(readAgentFilter);
  const [sidebarAgentFilter, setSidebarAgentFilterState] =
    useState<AgentFilter>("all");
  const [projectAgentFilter, setProjectAgentFilterState] =
    useState<AgentFilter>("all");
  const [scope, setScope] = useState<SearchScope>("global");
  const [includeCommands, setIncludeCommandsState] = useState<boolean>(() => {
    try {
      return localStorage.getItem(INCLUDE_COMMANDS_KEY) !== "false";
    } catch {
      return true;
    }
  });
  const [currentProject, setCurrentProjectState] = useState<string | null>(
    null
  );
  const [currentProjectName, setCurrentProjectName] = useState<string | null>(
    null
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [indexProgress, setIndexProgress] = useState<IndexProgress | null>(
    null
  );
  const [refreshing, setRefreshing] = useState(false);
  const refreshingRef = useRef(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const noticeTimer = useRef<number | null>(null);

  // 主题：跟随系统时监听系统配色变化
  useEffect(() => {
    let media: MediaQueryList | null = null;
    try {
      media = window.matchMedia("(prefers-color-scheme: dark)");
    } catch {
      return;
    }
    const handler = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", handler);
    return () => media?.removeEventListener("change", handler);
  }, []);

  const resolvedTheme: ResolvedTheme =
    theme === "system" ? (systemDark ? "dark" : "light") : theme;

  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolvedTheme === "dark");
  }, [resolvedTheme]);

  useEffect(() => {
    persist(THEME_KEY, theme);
  }, [theme]);

  // 索引进度事件：整个应用共享一个监听器
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    listen<IndexProgress>(INDEX_PROGRESS_EVENT, (event) => {
      setIndexProgress(event.payload.phase === "done" ? null : event.payload);
    })
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

  useEffect(() => {
    return () => {
      if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    };
  }, []);

  const showNotice = useCallback((next: Notice) => {
    if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    setNotice(next);
    if (next.kind === "info") {
      noticeTimer.current = window.setTimeout(
        () => setNotice(null),
        INFO_NOTICE_MS
      );
    }
  }, []);

  const dismissNotice = useCallback(() => {
    if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    setNotice(null);
  }, []);

  const setTheme = useCallback((mode: ThemeMode) => setThemeState(mode), []);

  const setIncludeCommands = useCallback((b: boolean) => {
    setIncludeCommandsState(b);
    persist(INCLUDE_COMMANDS_KEY, String(b));
  }, []);

  const setAgentFilter = useCallback((agent: AgentFilter) => {
    setAgentFilterState(agent);
    persist(AGENT_FILTER_KEY, agent);
  }, []);

  const setSidebarAgentFilter = useCallback((agent: AgentFilter) => {
    setSidebarAgentFilterState(agent);
  }, []);

  const setProjectAgentFilter = useCallback((agent: AgentFilter) => {
    setProjectAgentFilterState(agent);
  }, []);

  const setQuery = useCallback((nextQuery: string) => {
    setQueryState(nextQuery);
  }, []);

  const setCurrentProject = useCallback(
    (path: string | null, name: string | null = null) => {
      setCurrentProjectState(path);
      setCurrentProjectName(name);
      if (!path) setScope("global");
    },
    []
  );

  const openSettings = useCallback(() => setSettingsOpen(true), []);
  const closeSettings = useCallback(() => setSettingsOpen(false), []);
  const openImport = useCallback(() => setImportOpen(true), []);
  const closeImport = useCallback(() => setImportOpen(false), []);

  const refreshIndex = useCallback(
    async (full = false) => {
      if (refreshingRef.current) return;
      refreshingRef.current = true;
      setRefreshing(true);
      try {
        const meta = await api.refreshIndex(full);
        await queryClient.invalidateQueries();
        showNotice({
          kind: "info",
          key: meta.reparsedFiles > 0 || full ? "refreshDone" : "refreshNoChange",
          params: { count: meta.reparsedFiles },
        });
      } catch (error) {
        showNotice({
          kind: "error",
          key: "refreshFailed",
          params: { error: errMessage(error) },
        });
      } finally {
        refreshingRef.current = false;
        setRefreshing(false);
        setIndexProgress(null);
      }
    },
    [queryClient, showNotice]
  );

  const value = useMemo<Store>(
    () => ({
      theme,
      resolvedTheme,
      setTheme,
      agentFilter,
      setAgentFilter,
      sidebarAgentFilter,
      setSidebarAgentFilter,
      projectAgentFilter,
      setProjectAgentFilter,
      query,
      setQuery,
      scope,
      setScope,
      includeCommands,
      setIncludeCommands,
      currentProject,
      currentProjectName,
      setCurrentProject,
      settingsOpen,
      openSettings,
      closeSettings,
      importOpen,
      openImport,
      closeImport,
      indexProgress,
      refreshing,
      refreshIndex,
      notice,
      dismissNotice,
    }),
    [
      theme,
      resolvedTheme,
      setTheme,
      agentFilter,
      setAgentFilter,
      sidebarAgentFilter,
      setSidebarAgentFilter,
      projectAgentFilter,
      setProjectAgentFilter,
      query,
      setQuery,
      scope,
      includeCommands,
      setIncludeCommands,
      currentProject,
      currentProjectName,
      setCurrentProject,
      settingsOpen,
      openSettings,
      closeSettings,
      importOpen,
      openImport,
      closeImport,
      indexProgress,
      refreshing,
      refreshIndex,
      notice,
      dismissNotice,
    ]
  );

  return (
    <StoreContext.Provider value={value}>{children}</StoreContext.Provider>
  );
}

export function useStore(): Store {
  const v = useContext(StoreContext);
  if (!v) throw new Error("useStore 必须在 StoreProvider 内使用");
  return v;
}
