import { useCallback, useMemo, type KeyboardEvent } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Folder, Globe, Search, X } from "lucide-react";
import { useStore, type SearchScope } from "@/store";
import { useT } from "@/i18n";
import {
  buildSearchUrl,
  historyIndex,
  parseSearchParams,
  SEARCH_PATH,
  type SearchState,
} from "@/lib/search";
import { cn, isMac, pathBasename } from "@/lib/utils";

export const GLOBAL_SEARCH_INPUT_ID = "global-search-input";

/**
 * 顶栏搜索框。搜索页以 URL 为准：输入时用 replace 更新查询参数，
 * 从其他页面开始搜索时 push 一条 /search 记录，因此结果页可以用返回键回到。
 */
export function SearchBar() {
  const { query, setQuery, scope, setScope, currentProject, currentProjectName } =
    useStore();
  const t = useT();
  const location = useLocation();
  const navigate = useNavigate();
  const onSearchPage = location.pathname === SEARCH_PATH;
  const urlState = useMemo(
    () => parseSearchParams(location.search),
    [location.search]
  );

  const effectiveScope: SearchScope = onSearchPage ? urlState.scope : scope;
  const folderPath = onSearchPage
    ? (urlState.project ?? currentProject)
    : currentProject;
  const folderName = onSearchPage
    ? urlState.project
      ? pathBasename(urlState.project)
      : currentProjectName
    : currentProjectName;
  const folderAvailable = !!folderPath;

  const go = useCallback(
    (next: Partial<SearchState>, replace: boolean) => {
      const base: SearchState = onSearchPage
        ? urlState
        : {
            q: query,
            scope,
            project: scope === "folder" ? currentProject : null,
            agent: "all",
          };
      const merged: SearchState = { ...base, ...next };
      if (merged.scope === "folder" && !merged.project) merged.scope = "global";
      navigate(buildSearchUrl(merged), { replace });
    },
    [onSearchPage, urlState, query, scope, currentProject, navigate]
  );

  const leaveSearch = useCallback(() => {
    setQuery("");
    if (!onSearchPage) return;
    if (historyIndex() > 0) navigate(-1);
    else navigate("/", { replace: true });
  }, [onSearchPage, navigate, setQuery]);

  const onChange = (value: string) => {
    setQuery(value);
    if (value.trim()) go({ q: value }, onSearchPage);
    else leaveSearch();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (query) leaveSearch();
    else event.currentTarget.blur();
  };

  const chooseScope = (next: SearchScope) => {
    if (next === "folder" && !folderAvailable) return;
    if (onSearchPage) {
      go({ scope: next, project: next === "folder" ? folderPath : null }, true);
    } else {
      setScope(next);
    }
  };

  return (
    <div className="flex min-w-0 flex-1 items-center gap-1.5">
      <div className="relative min-w-0 flex-1">
        <Search
          size={15}
          className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
        />
        <input
          id={GLOBAL_SEARCH_INPUT_ID}
          value={query}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder={
            effectiveScope === "folder" && folderName
              ? t("searchInFolderPlaceholder", { name: folderName })
              : t("searchAllPlaceholder")
          }
          className="h-9 w-full rounded-lg border border-border bg-surface-2/60 pl-9 pr-12 text-[13px] text-foreground outline-none transition-colors placeholder:text-muted focus:border-accent focus:bg-surface focus:ring-2 focus:ring-ring/20"
        />
        {query ? (
          <button
            type="button"
            onClick={leaveSearch}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted transition-colors hover:text-foreground"
            title={t("clearSearch")}
          >
            <X size={15} />
          </button>
        ) : (
          <kbd
            aria-hidden
            className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 rounded border border-border bg-surface px-1.5 py-0.5 font-sans text-[10px] leading-none text-muted"
          >
            {isMac ? "⌘K" : "Ctrl K"}
          </kbd>
        )}
      </div>

      <div className="flex shrink-0 items-center rounded-lg border border-border bg-background p-0.5">
        <button
          type="button"
          onClick={() => chooseScope("global")}
          title={t("scopeGlobal")}
          className={cn(
            "flex h-7 items-center gap-1 rounded-md px-2.5 text-xs font-medium transition-colors max-[1220px]:w-7 max-[1220px]:justify-center max-[1220px]:px-0",
            effectiveScope === "global"
              ? "bg-accent text-accent-fg"
              : "text-muted hover:text-foreground"
          )}
        >
          <Globe size={13} />
          <span className="max-[1220px]:hidden">{t("scopeGlobal")}</span>
        </button>
        <button
          type="button"
          disabled={!folderAvailable}
          onClick={() => chooseScope("folder")}
          title={
            folderAvailable ? t("scopeFolder") : t("scopeFolderDisabledTitle")
          }
          className={cn(
            "flex h-7 items-center gap-1 rounded-md px-2.5 text-xs font-medium transition-colors max-[1220px]:w-7 max-[1220px]:justify-center max-[1220px]:px-0",
            effectiveScope === "folder"
              ? "bg-accent text-accent-fg"
              : "text-muted hover:text-foreground",
            !folderAvailable && "cursor-not-allowed opacity-40"
          )}
        >
          <Folder size={13} />
          <span className="max-[1220px]:hidden">{t("scopeFolder")}</span>
        </button>
      </div>
    </div>
  );
}
