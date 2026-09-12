import { useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Check, Download, FolderOpen, SearchX } from "lucide-react";
import { useStore } from "@/store";
import { useSearch } from "@/hooks/queries";
import { PromptList, type PromptListItem } from "@/components/PromptList";
import { AgentFilterControl } from "@/components/AgentBadge";
import { Button, CenterMessage, Spinner } from "@/components/ui";
import { api, errMessage } from "@/lib/api";
import { getCurrentLang, useT } from "@/i18n";
import { buildSearchUrl, parseSearchParams } from "@/lib/search";
import { formatNumber, pathBasename } from "@/lib/utils";
import type { AgentFilter, ExportResult } from "@/lib/types";

/** 搜索结果页。所有状态来自 URL 查询参数，返回键与刷新都能恢复。 */
export function SearchResults() {
  const { includeCommands, setQuery } = useStore();
  const t = useT();
  const location = useLocation();
  const navigate = useNavigate();
  const urlState = useMemo(
    () => parseSearchParams(location.search),
    [location.search]
  );
  const { q, scope, project, agent } = urlState;

  // URL 变化（输入、返回键、直接打开链接）时同步顶栏输入框
  useEffect(() => {
    setQuery(q);
  }, [q, setQuery]);

  const projectFilter = scope === "folder" ? project : null;
  const folderName = projectFilter ? pathBasename(projectFilter) : null;
  const { data, isLoading, isError, error, debouncedQuery } = useSearch(
    q,
    projectFilter,
    includeCommands,
    agent
  );

  const setAgent = (next: AgentFilter) =>
    navigate(buildSearchUrl({ ...urlState, agent: next }), { replace: true });

  // memo 保持引用稳定：PromptList 以 items 引用变化作为重置分批的信号
  const items: PromptListItem[] = useMemo(
    () => (data ?? []).map((r) => ({ entry: r.entry, ranges: r.matchRanges })),
    [data]
  );

  // 批量导出当前搜索结果
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  useEffect(() => {
    setExportResult(null);
    setExportError(null);
  }, [agent, debouncedQuery, includeCommands, projectFilter]);

  const handleExport = async () => {
    if (!debouncedQuery || exporting) return;
    setExporting(true);
    setExportError(null);
    setExportResult(null);
    try {
      const res = await api.exportSearchResults({
        query: debouncedQuery,
        projectFilter,
        includeCommands,
        agentFilter: agent,
        write: true,
        lang: getCurrentLang(),
      });
      setExportResult(res);
    } catch (e) {
      setExportError(errMessage(e));
    } finally {
      setExporting(false);
    }
  };

  const revealExported = async () => {
    if (exportResult?.path) {
      try {
        await api.revealPath(exportResult.path);
      } catch {
        /* 文件可能被移动，忽略 */
      }
    }
  };

  return (
    <div className="page-content pb-10">
      <header className="sticky top-0 z-10 -mx-2 mb-3 flex min-w-0 items-start justify-between gap-4 border-b border-border bg-background/95 px-2 py-4 backdrop-blur-sm">
        <div className="min-w-0">
          <h1 className="text-lg font-semibold text-foreground">
            {t("searchResultsTitle")}
          </h1>
          <p className="mt-0.5 truncate text-xs text-muted">
            {folderName
              ? t("searchInFolder", { name: folderName })
              : t("globalSearch")}
            {debouncedQuery &&
              ` · ${t("searchKeyword", { keyword: debouncedQuery })}`}
            {data &&
              ` · ${t("searchHits", { count: formatNumber(data.length) })}`}
          </p>
        </div>
        <div className="flex shrink-0 items-end gap-2">
          <div className="flex items-center gap-2">
            <span className="text-xs font-medium text-muted max-[1080px]:hidden">
              {t("searchAgentSource")}
            </span>
            <AgentFilterControl
              value={agent}
              onChange={setAgent}
              ariaLabel={t("searchAgentSource")}
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={handleExport}
            disabled={exporting || items.length === 0}
          >
            {exporting ? (
              <Spinner className="border-accent/40 border-t-accent" />
            ) : (
              <Download size={13} />
            )}
            {t("exportSearchResults")}
          </Button>
        </div>
      </header>

      {exportError && (
        <p className="mb-3 text-xs text-danger">
          {t("exportFailed", { error: exportError })}
        </p>
      )}
      {exportResult?.path && (
        <div className="mb-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
          <Check size={13} className="shrink-0 text-success" />
          <span className="text-foreground">
            {t("exportedCountTo", {
              count: formatNumber(exportResult.promptCount),
            })}{" "}
            <span className="font-medium" title={exportResult.path}>
              {pathBasename(exportResult.path)}
            </span>
          </span>
          <button
            onClick={revealExported}
            className="flex items-center gap-1 text-accent transition-colors hover:underline"
          >
            <FolderOpen size={12} />
            {t("revealInFinder")}
          </button>
        </div>
      )}

      {!debouncedQuery ? (
        <CenterMessage
          icon={<SearchX size={28} />}
          title={t("searchEmptyQuery")}
        />
      ) : isLoading ? (
        <CenterMessage
          icon={<Spinner className="h-6 w-6" />}
          title={t("searching")}
        />
      ) : isError ? (
        <CenterMessage
          icon={<SearchX size={28} />}
          title={t("searchFailed")}
          hint={errMessage(error)}
        />
      ) : items.length === 0 ? (
        <CenterMessage
          icon={<SearchX size={28} />}
          title={t("noMatchingPrompts")}
          hint={t("noMatchingPromptsHint")}
        />
      ) : (
        <PromptList
          items={items}
          showProject={scope === "global"}
          showAgentBadge={agent === "all"}
        />
      )}
    </div>
  );
}
