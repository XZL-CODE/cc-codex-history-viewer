import { useEffect } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  AlertCircle,
  Info,
  Languages,
  Layers3,
  RefreshCw,
  Settings,
  Terminal,
  X,
} from "lucide-react";
import { useStore } from "@/store";
import { useLang, useT } from "@/i18n";
import { cn, decodePath, isMac, modKeyLabel, pathBasename } from "@/lib/utils";
import { SEARCH_PATH } from "@/lib/search";
import { IndexProgressBar, indexProgressLabel } from "./IndexProgress";
import { GLOBAL_SEARCH_INPUT_ID, SearchBar } from "./SearchBar";
import { SettingsDialog } from "./SettingsDialog";
import { Sidebar } from "./Sidebar";
import { ThemeToggle } from "./ThemeToggle";
import { Button } from "./ui";

/** 右下角提示：刷新结果 4 秒后自动消失，错误保留到手动关闭。 */
function NoticeToast() {
  const { notice, dismissNotice } = useStore();
  const t = useT();
  if (!notice) return null;
  const isError = notice.kind === "error";
  return (
    <div
      role="status"
      className={cn(
        "fixed bottom-4 right-4 z-40 flex max-w-md items-start gap-2 rounded-lg border bg-surface px-3 py-2.5 text-xs shadow-lg",
        isError ? "border-danger/40 text-danger" : "border-border text-foreground"
      )}
    >
      {isError ? (
        <AlertCircle size={14} className="mt-0.5 shrink-0" />
      ) : (
        <Info size={14} className="mt-0.5 shrink-0 text-accent" />
      )}
      <span className="min-w-0 break-words">{t(notice.key, notice.params)}</span>
      <button
        type="button"
        onClick={dismissNotice}
        title={t("dismiss")}
        className="ml-1 shrink-0 rounded p-0.5 text-muted transition-colors hover:bg-surface-2 hover:text-foreground"
      >
        <X size={13} />
      </button>
    </div>
  );
}

export function Layout() {
  const {
    includeCommands,
    setIncludeCommands,
    setQuery,
    setCurrentProject,
    setProjectAgentFilter,
    setScope,
    settingsOpen,
    openSettings,
    closeSettings,
    refreshing,
    refreshIndex,
    indexProgress,
  } = useStore();
  const location = useLocation();
  const navigate = useNavigate();
  const t = useT();
  const { lang, setLang } = useLang();

  // 路由进入新文件夹时登记搜索范围并重置详情筛选；离开搜索页时清空搜索框。
  useEffect(() => {
    const match = location.pathname.match(/^\/project\/(.+)$/);
    if (match) {
      const path = decodePath(match[1]);
      setCurrentProject(path, pathBasename(path));
      setProjectAgentFilter("all");
      setScope("folder");
    } else {
      setCurrentProject(null);
    }
    if (location.pathname !== SEARCH_PATH) setQuery("");
  }, [
    location.pathname,
    setCurrentProject,
    setProjectAgentFilter,
    setScope,
    setQuery,
  ]);

  // 全局快捷键：⌘/Ctrl+K 聚焦搜索，⌘/Ctrl+R 或 F5 增量刷新，⌘/Ctrl+, 打开设置，Esc 关闭设置。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = isMac ? event.metaKey : event.ctrlKey;
      const key = event.key.toLowerCase();
      if (mod && key === "k") {
        event.preventDefault();
        const input = document.getElementById(
          GLOBAL_SEARCH_INPUT_ID
        ) as HTMLInputElement | null;
        input?.focus();
        input?.select();
        return;
      }
      if ((mod && key === "r") || event.key === "F5") {
        event.preventDefault();
        void refreshIndex(false);
        return;
      }
      if (mod && event.key === ",") {
        event.preventDefault();
        openSettings();
        return;
      }
      if (event.key === "Escape" && settingsOpen) {
        closeSettings();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [refreshIndex, openSettings, closeSettings, settingsOpen]);

  const busy = refreshing || indexProgress !== null;
  const shortcut = (key: string) => `${modKeyLabel}${isMac ? "" : "+"}${key}`;

  return (
    <div className="grid h-screen min-w-0 grid-rows-[56px_minmax(0,1fr)]">
      <header className="grid min-w-0 grid-cols-[264px_minmax(0,1fr)] border-b border-border bg-surface max-[1220px]:grid-cols-[248px_minmax(0,1fr)]">
        <button
          type="button"
          onClick={() => {
            setQuery("");
            navigate("/");
          }}
          className="flex min-w-0 items-center gap-2.5 px-3.5 text-left transition-colors hover:bg-surface-2/60"
          title={t("backHome")}
          aria-label={t("backHome")}
        >
          <span className="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-lg bg-accent text-accent-fg">
            <Layers3 size={17} />
          </span>
          <span className="truncate text-sm font-semibold text-foreground max-[1080px]:hidden">
            Coding Agent History Viewer
          </span>
        </button>

        <div className="flex min-w-0 items-center gap-1.5 px-3 py-2">
          <div className="min-w-0 flex-1">
            <SearchBar />
          </div>

          {indexProgress && (
            <span className="hidden shrink-0 text-[11px] text-muted min-[1220px]:inline">
              {indexProgressLabel(indexProgress, t)}
            </span>
          )}

          <button
            type="button"
            onClick={() => setIncludeCommands(!includeCommands)}
            title={
              includeCommands
                ? t("commandsShownTitle")
                : t("commandsHiddenTitle")
            }
            className={cn(
              "flex h-9 shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors",
              includeCommands
                ? "border-accent/40 bg-accent/15 text-accent"
                : "border-border text-muted hover:bg-surface-2 hover:text-foreground"
            )}
          >
            <Terminal size={14} />
            <span className="max-[1220px]:hidden">{t("commandsToggle")}</span>
          </button>

          <Button
            variant="ghost"
            size="icon"
            onClick={() => void refreshIndex(false)}
            disabled={busy}
            title={t("refreshTitle", { shortcut: shortcut("R") })}
          >
            <RefreshCw size={16} className={cn(busy && "animate-spin")} />
          </Button>

          <Button
            variant="ghost"
            size="icon"
            onClick={openSettings}
            title={t("settingsButtonTitle", { shortcut: shortcut(",") })}
          >
            <Settings size={16} />
          </Button>

          <button
            type="button"
            onClick={() => setLang(lang === "zh" ? "en" : "zh")}
            title={t("switchLanguage")}
            className="flex h-9 shrink-0 items-center gap-1 rounded-lg border border-border px-2 text-xs font-medium text-muted transition-colors hover:bg-surface-2 hover:text-foreground"
          >
            <Languages size={14} />
            <span className="max-[1220px]:hidden">{t("langBadge")}</span>
          </button>

          <ThemeToggle />
        </div>
      </header>

      <SettingsDialog open={settingsOpen} onClose={closeSettings} />

      <div className="grid min-h-0 min-w-0 grid-cols-[264px_minmax(0,1fr)] max-[1220px]:grid-cols-[248px_minmax(0,1fr)]">
        <Sidebar />
        <div className="relative min-h-0 min-w-0">
          <IndexProgressBar />
          <main className="h-full min-h-0 min-w-0 overflow-y-auto bg-background">
            <Outlet />
          </main>
        </div>
      </div>

      <NoticeToast />
    </div>
  );
}
