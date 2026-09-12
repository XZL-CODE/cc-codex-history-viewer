import { useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  Check,
  Download,
  FileSearch,
  FolderOpen,
  ListTree,
  SearchX,
} from "lucide-react";
import { useStore } from "@/store";
import { useConversationSearch, useSearch } from "@/hooks/queries";
import { PromptList, type PromptListItem } from "@/components/PromptList";
import { ConversationHits } from "@/components/ConversationHits";
import { AgentFilterControl } from "@/components/AgentBadge";
import { Button, CenterMessage, Spinner } from "@/components/ui";
import { api, errMessage } from "@/lib/api";
import { getCurrentLang, useT } from "@/i18n";
import {
  buildSearchUrl,
  parseSearchParams,
  type SearchMode,
  type SearchState,
} from "@/lib/search";
import { cn, formatNumber, pathBasename } from "@/lib/utils";
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
  const { q, scope, project, agent, mode } = urlState;

  // URL 变化（输入、返回键、直接打开链接）时同步顶栏输入框
  useEffect(() => {
    setQuery(q);
  }, [q, setQuery]);

  const projectFilter = scope === "folder" ? project : null;
  const folderName = projectFilter ? pathBasename(projectFilter) : null;
  const promptsQ = useSearch(q, projectFilter, includeCommands, agent);
  const contentQ = useConversationSearch(
    q,
    projectFilter,
    agent,
    mode === "content"
  );
  const debouncedQuery =
    mode === "content" ? contentQ.debouncedQuery : promptsQ.debouncedQuery;

  const update = (next: Partial<SearchState>) =>
    navigate(buildSearchUrl({ ...urlState, ...next }), { replace: true });
  const setAgent = (next: AgentFilter) => update({ agent: next });
  const setMode = (next: SearchMode) => update({ mode: next });

  // memo 保持引用稳定：PromptList 以 items 引用变化作为重置分批的信号
  const items: PromptListItem[] = useMemo(
    () =>
      (promptsQ.data ?? []).map((r) => ({
        entry: r.entry,
        ranges: r.matchRanges,
      })),
    [promptsQ.data]
  );

  // 批量导出当前搜索结果（仅 Prompt 模式）
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  useEffect(() => {
    setExportResult(null);
    setExportError(null);
  }, [agent, debouncedQuery, includeCommands, projectFilter, mode]);

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

  const summary = (() => {
    const parts = [
      folderName ? t("searchInFolder", { name: folderName }) : t("globalSearch"),
    ];
    if (debouncedQuery) parts.push(t("searchKeyword", { keyword: debouncedQuery }));
    if (mode === "prompts" && promptsQ.data) {
      parts.push(t("searchHits", { count: formatNumber(promptsQ.data.length) }));
    }
    if (mode === "content" && contentQ.data) {
      parts.push(
        t("contentSummary", {
          files: formatNumber(contentQ.data.scannedFiles),
          sessions: formatNumber(contentQ.data.matchedSessions),
          hits: formatNumber(contentQ.data.hits.length),
          ms: formatNumber(contentQ.data.elapsedMs),
        })
      );
    }
    return parts.join(" · ");
  })();

  const modes: { value: SearchMode; icon: typeof ListTree; labelKey: "searchModePrompts" | "searchModeContent" }[] = [
    { value: "prompts", icon: ListTree, labelKey: "searchModePrompts" },
    { value: "content", icon: FileSearch, labelKey: "searchModeContent" },
  ];

  return (
    <div className="page-content pb-10">
      <header className="sticky top-0 z-10 -mx-2 mb-3 flex min-w-0 items-start justify-between gap-4 border-b border-border bg-background/95 px-2 py-4 backdrop-blur-sm">
        <div className="min-w-0">
          <h1 className="text-lg font-semibold text-foreground">
            {t("searchResultsTitle")}
          </h1>
          <p className="mt-0.5 truncate text-xs text-muted">{summary}</p>
          <div
            className="mt-2 inline-flex items-center rounded-lg border border-border bg-surface p-0.5"
            role="group"
            aria-label={t("searchModeLabel")}
          >
            {modes.map((option) => {
              const Icon = option.icon;
              return (
                <button
                  key={option.value}
                  type="button"
                  aria-pressed={mode === option.value}
                  onClick={() => setMode(option.value)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors",
                    mode === option.value
                      ? "bg-accent text-accent-fg"
                      : "text-muted hover:text-foreground"
                  )}
                >
                  <Icon size={13} />
                  {t(option.labelKey)}
                </button>
              );
            })}
          </div>
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
          {mode === "prompts" && (
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
          )}
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
      ) : mode === "content" ? (
        contentQ.isLoading || contentQ.isFetching ? (
          <CenterMessage
            icon={<Spinner className="h-6 w-6" />}
            title={t("contentScanning")}
            hint={t("contentScanningHint")}
          />
        ) : contentQ.isError ? (
          <CenterMessage
            icon={<SearchX size={28} />}
            title={t("searchFailed")}
            hint={errMessage(contentQ.error)}
          />
        ) : !contentQ.data || contentQ.data.hits.length === 0 ? (
          <CenterMessage
            icon={<SearchX size={28} />}
            title={t("contentNoHits")}
            hint={t("contentNoHitsHint")}
          />
        ) : (
          <>
            {contentQ.data.truncated && (
              <p className="mb-3 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-xs text-warning">
                {t("contentTruncated")}
              </p>
            )}
            <ConversationHits
              hits={contentQ.data.hits}
              query={debouncedQuery}
              showProject={scope === "global"}
              showAgentBadge={agent === "all"}
            />
          </>
        )
      ) : promptsQ.isLoading ? (
        <CenterMessage
          icon={<Spinner className="h-6 w-6" />}
          title={t("searching")}
        />
      ) : promptsQ.isError ? (
        <CenterMessage
          icon={<SearchX size={28} />}
          title={t("searchFailed")}
          hint={errMessage(promptsQ.error)}
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
