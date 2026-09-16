import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Link, useParams } from "react-router-dom";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Check,
  Clock,
  Download,
  Folder,
  FolderOpen,
  GitBranch,
  ListChecks,
  ListTree,
  MessagesSquare,
} from "lucide-react";
import { useStore } from "@/store";
import {
  useProjectPrompts,
  useProjectSessions,
  useProjects,
} from "@/hooks/queries";
import { PromptList } from "@/components/PromptList";
import {
  Badge,
  Button,
  CenterMessage,
  Skeleton,
  Spinner,
} from "@/components/ui";
import { getCurrentLang, useT, type DictKey } from "@/i18n";
import type {
  ExportProgress,
  SessionRef,
  SessionsExportResult,
  SessionSummary,
  SortMode,
} from "@/lib/types";
import {
  absoluteTime,
  cn,
  formatDuration,
  formatNumber,
  formatTokens,
  formatUsageCost,
  pathBasename,
} from "@/lib/utils";
import { api, errMessage } from "@/lib/api";
import { AgentBadge, AgentFilterControl } from "@/components/AgentBadge";

/** 后端批量导出进度事件名，与 Rust `EXPORT_PROGRESS_EVENT` 一致 */
const EXPORT_PROGRESS_EVENT = "export-progress";

const sortOptions: { value: SortMode; labelKey: DictKey }[] = [
  { value: "newest", labelKey: "sortNewest" },
  { value: "oldest", labelKey: "sortOldest" },
  { value: "longest", labelKey: "sortLongest" },
];

type SessionSort = "newest" | "cost" | "messages" | "duration";

const sessionSortOptions: { value: SessionSort; labelKey: DictKey }[] = [
  { value: "newest", labelKey: "sortNewest" },
  { value: "cost", labelKey: "sortByCost" },
  { value: "messages", labelKey: "sortByMessages" },
  { value: "duration", labelKey: "sortByDuration" },
];

const layoutOptions: { merge: boolean; labelKey: DictKey }[] = [
  { merge: false, labelKey: "exportOnePerSession" },
  { merge: true, labelKey: "exportMergedFile" },
];

/** 选择集的键：同一 session ID 可能同时存在于两个产品，必须带 agent */
function sessionKey(session: SessionRef): string {
  return `${session.agent}:${session.sessionId}`;
}

function sessionDuration(session: SessionSummary): number {
  return Math.max(session.endedAt - session.startedAt, 0);
}

function sortSessions(list: SessionSummary[], sort: SessionSort) {
  const sorted = [...list];
  switch (sort) {
    case "cost":
      sorted.sort(
        (a, b) =>
          b.usage.estCostUsd - a.usage.estCostUsd ||
          b.usage.totalTokensIncludingCache - a.usage.totalTokensIncludingCache
      );
      break;
    case "messages":
      sorted.sort((a, b) => b.messageCount - a.messageCount);
      break;
    case "duration":
      sorted.sort((a, b) => sessionDuration(b) - sessionDuration(a));
      break;
    default:
      sorted.sort((a, b) => b.startedAt - a.startedAt);
  }
  return sorted;
}

function ListSkeleton() {
  return (
    <div className="space-y-2.5">
      {Array.from({ length: 6 }).map((_, i) => (
        <Skeleton key={i} className="h-20 w-full" />
      ))}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  children,
}: {
  active: boolean;
  onClick: () => void;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors",
        active
          ? "bg-accent text-accent-fg"
          : "text-muted hover:text-foreground"
      )}
    >
      {icon}
      {children}
    </button>
  );
}

function SortControl<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; labelKey: DictKey }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-1 rounded-lg border border-border bg-surface p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
            value === option.value
              ? "bg-accent text-accent-fg"
              : "text-muted hover:text-foreground"
          )}
        >
          {t(option.labelKey)}
        </button>
      ))}
    </div>
  );
}

function SessionRowContent({
  session,
  showAgentBadge,
}: {
  session: SessionSummary;
  showAgentBadge: boolean;
}) {
  const t = useT();
  const duration = sessionDuration(session);
  const tokens = session.usage.totalTokensIncludingCache;
  return (
    <>
      <div className="line-clamp-2 text-sm font-medium text-foreground">
        {session.title || t("untitledSession")}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
        {showAgentBadge && <AgentBadge agent={session.agent} />}
        <span>{absoluteTime(session.startedAt)}</span>
        <span>
          {t("messagesCount", { count: formatNumber(session.messageCount) })}
        </span>
        {duration > 0 && (
          <span className="flex items-center gap-1">
            <Clock size={11} />
            {formatDuration(duration)}
          </span>
        )}
        {tokens > 0 && (
          <span
            className="font-medium text-foreground"
            title={t("tokenTotalSuffix", { value: formatNumber(tokens) })}
          >
            {t("tokensUnit", { value: formatTokens(tokens) })} ·{" "}
            {formatUsageCost(session.usage)}
          </span>
        )}
        {session.gitBranch && (
          <span className="flex items-center gap-1">
            <GitBranch size={11} />
            {session.gitBranch}
          </span>
        )}
        {session.cliVersion && (
          <Badge tone="muted">CLI {session.cliVersion}</Badge>
        )}
        {session.models.length > 0 && (
          <span
            className="max-w-[min(100%,20rem)] truncate"
            title={session.models.join(", ")}
          >
            {session.models.join(", ")}
          </span>
        )}
        {session.source && <Badge tone="outline">{session.source}</Badge>}
      </div>
    </>
  );
}

const rowClass =
  "block rounded-lg border bg-surface p-3.5 transition-[border-color,box-shadow] hover:border-accent/40 hover:shadow-sm";

/** 普通模式是进入详情的链接；选择模式下整行变成一个复选框 */
function SessionRow({
  session,
  showAgentBadge,
  selectable,
  selected,
  onToggle,
}: {
  session: SessionSummary;
  showAgentBadge: boolean;
  selectable: boolean;
  selected: boolean;
  onToggle: () => void;
}) {
  if (!selectable) {
    return (
      <Link
        to={`/conversation/${session.agent}/${session.sessionId}`}
        className={cn(rowClass, "border-border")}
      >
        <SessionRowContent session={session} showAgentBadge={showAgentBadge} />
      </Link>
    );
  }
  return (
    <div
      role="checkbox"
      aria-checked={selected}
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(event) => {
        if (event.key === " " || event.key === "Enter") {
          event.preventDefault();
          onToggle();
        }
      }}
      className={cn(
        rowClass,
        "flex cursor-pointer select-none items-start gap-3 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        selected ? "border-accent/60 bg-accent/5" : "border-border"
      )}
    >
      <input
        type="checkbox"
        checked={selected}
        readOnly
        tabIndex={-1}
        className="pointer-events-none mt-0.5 shrink-0 accent-[var(--accent)]"
      />
      <div className="min-w-0 flex-1">
        <SessionRowContent session={session} showAgentBadge={showAgentBadge} />
      </div>
    </div>
  );
}

/** 选择模式的工具条：计数、全选 / 清除，以及批量导出面板（文件组织、是否含执行过程、进度与结果） */
function SessionsExportBar({
  project,
  sessions,
  selected,
  onSelectAll,
  onClear,
}: {
  project: string;
  sessions: SessionSummary[];
  selected: Set<string>;
  onSelectAll: () => void;
  onClear: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [merge, setMerge] = useState(false);
  const [includeTools, setIncludeTools] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [result, setResult] = useState<SessionsExportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const picked = useMemo(
    () => sessions.filter((session) => selected.has(sessionKey(session))),
    [sessions, selected]
  );
  const messageTotal = useMemo(
    () => picked.reduce((sum, session) => sum + session.messageCount, 0),
    [picked]
  );

  // 选择或选项一变，上一次的结果就不再对应当前配置
  useEffect(() => {
    setResult(null);
    setError(null);
  }, [selected, merge, includeTools]);

  // 导出期间监听后端的解析进度
  useEffect(() => {
    if (!exporting) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    listen<ExportProgress>(EXPORT_PROGRESS_EVENT, (event) => {
      setProgress(event.payload);
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        /* 非 Tauri 环境没有事件通道 */
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [exporting]);

  const handleExport = async () => {
    if (exporting || picked.length === 0) return;
    const targets: SessionRef[] = picked.map((session) => ({
      agent: session.agent,
      sessionId: session.sessionId,
    }));
    setExporting(true);
    setError(null);
    setResult(null);
    setProgress({ done: 0, total: targets.length });
    try {
      const response = await api.exportSessions({
        project,
        sessions: targets,
        merge,
        includeTools,
        lang: getCurrentLang(),
      });
      setResult(response);
    } catch (exportError) {
      setError(errMessage(exportError));
    } finally {
      setExporting(false);
      setProgress(null);
    }
  };

  const reveal = async () => {
    if (!result) return;
    try {
      await api.revealPath(result.path);
    } catch {
      /* 文件可能已被移动 */
    }
  };

  const determinate = exporting && progress !== null && progress.total > 0;

  return (
    <div className="mb-3 rounded-lg border border-border bg-surface">
      <div className="flex flex-wrap items-center gap-2 px-3 py-2">
        <span className="text-xs font-medium text-foreground">
          {t("selectedOfTotal", {
            count: formatNumber(picked.length),
            total: formatNumber(sessions.length),
          })}
        </span>
        <Button
          variant="ghost"
          size="sm"
          onClick={onSelectAll}
          disabled={picked.length === sessions.length}
        >
          {t("selectAll")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClear}
          disabled={picked.length === 0}
        >
          {t("clearSelection")}
        </Button>
        <div className="flex-1" />
        <Button
          size="sm"
          variant={open ? "subtle" : "primary"}
          onClick={() => setOpen((value) => !value)}
          disabled={!open && picked.length === 0}
          title={t("exportSelectedTitle")}
        >
          <Download size={13} />
          {t("exportSelected")}
          {picked.length > 0 ? ` (${formatNumber(picked.length)})` : ""}
        </Button>
      </div>

      {open && (
        <div className="space-y-2.5 border-t border-border px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-muted">
                {t("exportLayoutLabel")}
              </span>
              <div className="flex items-center rounded-lg border border-border bg-background p-0.5">
                {layoutOptions.map((option) => (
                  <button
                    key={option.labelKey}
                    type="button"
                    onClick={() => setMerge(option.merge)}
                    className={cn(
                      "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                      merge === option.merge
                        ? "bg-accent text-accent-fg"
                        : "text-muted hover:text-foreground"
                    )}
                  >
                    {t(option.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <label className="flex cursor-pointer select-none items-center gap-1.5 text-xs text-foreground">
              <input
                type="checkbox"
                checked={includeTools}
                onChange={(event) => setIncludeTools(event.target.checked)}
                className="accent-[var(--accent)]"
              />
              {t("includeToolsLabel")}
            </label>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-xs text-muted">
              {picked.length === 0
                ? t("noSessionSelected")
                : t("willExportSessions", {
                    sessions: formatNumber(picked.length),
                    messages: formatNumber(messageTotal),
                  })}
            </span>
            <Button
              size="sm"
              onClick={handleExport}
              disabled={exporting || picked.length === 0}
            >
              {exporting ? (
                <Spinner className="border-accent-fg/40 border-t-accent-fg" />
              ) : (
                <Download size={13} />
              )}
              {exporting
                ? determinate
                  ? t("exportingProgress", {
                      done: formatNumber(progress.done),
                      total: formatNumber(progress.total),
                    })
                  : t("exporting")
                : t("confirmExport")}
            </Button>
          </div>

          {determinate && (
            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={progress.total}
              aria-valuenow={progress.done}
              className="h-1 overflow-hidden rounded bg-accent/15"
            >
              <div
                className="h-full bg-accent transition-[width] duration-150"
                style={{
                  width: `${Math.min(100, (progress.done / progress.total) * 100)}%`,
                }}
              />
            </div>
          )}

          {error && (
            <p className="text-xs text-danger">{t("exportFailed", { error })}</p>
          )}
          {result && (
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
              <Check size={13} className="shrink-0 text-success" />
              <span className="text-foreground">
                {t("exportedSessionsTo", {
                  count: formatNumber(result.sessionCount),
                })}{" "}
                <span className="font-medium" title={result.path}>
                  {pathBasename(result.path)}
                </span>
                {!result.merged && (
                  <span className="text-muted">
                    {" "}
                    {t("exportedFolderNote", {
                      files: formatNumber(result.fileCount),
                    })}
                  </span>
                )}
              </span>
              <button
                onClick={reveal}
                className="flex items-center gap-1 text-accent transition-colors hover:underline"
              >
                <FolderOpen size={12} />
                {t("revealInFinder")}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function ProjectPrompts() {
  const params = useParams();
  const projectPath = params.encoded ?? "";
  const name = pathBasename(projectPath);

  // 「当前文件夹」由 Layout 根据路由统一登记
  const { includeCommands, projectAgentFilter, setProjectAgentFilter } =
    useStore();
  const t = useT();
  const [sort, setSort] = useState<SortMode>("newest");
  const [sessionSort, setSessionSort] = useState<SessionSort>("newest");
  const [tab, setTab] = useState<"prompts" | "sessions">("prompts");

  // 会话选择模式：选择集只对当前列表有效
  const [selecting, setSelecting] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());

  const projectsQ = useProjects(projectAgentFilter);
  const info = projectsQ.data?.find((p) => p.path === projectPath);
  const promptsQ = useProjectPrompts(
    projectPath,
    sort,
    includeCommands,
    projectAgentFilter
  );
  const sessionsQ = useProjectSessions(
    tab === "sessions" ? projectPath : null,
    projectAgentFilter
  );

  // memo 保持引用稳定：PromptList 以 items 引用变化作为重置分批的信号
  const promptItems = useMemo(
    () => (promptsQ.data ?? []).map((entry) => ({ entry })),
    [promptsQ.data]
  );
  const sessions = useMemo(
    () => sortSessions(sessionsQ.data ?? [], sessionSort),
    [sessionsQ.data, sessionSort]
  );

  // 切换文件夹或离开会话标签页时退出选择模式
  useEffect(() => {
    setSelecting(false);
    setSelected(new Set());
  }, [projectPath]);
  useEffect(() => {
    if (tab !== "sessions") {
      setSelecting(false);
      setSelected(new Set());
    }
  }, [tab]);
  // 列表变化（如切换来源筛选）后，丢掉已不在列表里的选择
  useEffect(() => {
    const keys = new Set((sessionsQ.data ?? []).map(sessionKey));
    setSelected((previous) => {
      const next = new Set([...previous].filter((key) => keys.has(key)));
      return next.size === previous.size ? previous : next;
    });
  }, [sessionsQ.data]);

  const toggleSelecting = () => {
    setSelecting((value) => !value);
    setSelected(new Set());
  };
  const toggleSelected = useCallback((key: string) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  return (
    <div className="page-content py-6">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Folder size={18} className="shrink-0 text-accent" />
            <h1 className="min-w-0 break-words text-xl font-semibold text-foreground">
              {name}
            </h1>
          </div>
          <p className="mt-1 break-all text-xs text-muted">{projectPath}</p>
          {info && (
            <div className="mt-2.5 flex flex-wrap items-center gap-2">
              <Badge tone="accent">
                {t("promptCountLabel", {
                  count: formatNumber(info.promptCount),
                })}
              </Badge>
              {info.commandCount > 0 && (
                <Badge tone="muted">
                  {t("commandCountLabel", {
                    count: formatNumber(info.commandCount),
                  })}
                </Badge>
              )}
              {info.hasConversations && (
                <Badge tone="muted">
                  {t("sessionCountLabel", {
                    count: formatNumber(info.sessionCount),
                  })}
                </Badge>
              )}
              {info.agents.map((agent) => (
                <AgentBadge key={agent} agent={agent} />
              ))}
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-xs font-medium text-muted max-[1080px]:hidden">
            {t("projectAgentSource")}
          </span>
          <AgentFilterControl
            value={projectAgentFilter}
            onChange={setProjectAgentFilter}
            ariaLabel={t("projectAgentSource")}
          />
        </div>
      </div>

      <div className="sticky top-0 z-10 -mx-2 mb-3 flex items-center justify-between gap-2 border-b border-border bg-background/95 px-2 py-3 backdrop-blur-sm">
        <div className="flex items-center rounded-lg border border-border bg-surface p-0.5">
          <TabButton
            active={tab === "prompts"}
            onClick={() => setTab("prompts")}
            icon={<ListTree size={13} />}
          >
            {t("promptsTab")}
          </TabButton>
          <TabButton
            active={tab === "sessions"}
            onClick={() => setTab("sessions")}
            icon={<MessagesSquare size={13} />}
          >
            {t("sessionsTab")}
          </TabButton>
        </div>

        {tab === "prompts" ? (
          <SortControl options={sortOptions} value={sort} onChange={setSort} />
        ) : (
          <div className="flex items-center gap-2">
            <SortControl
              options={sessionSortOptions}
              value={sessionSort}
              onChange={setSessionSort}
            />
            {sessions.length > 0 && (
              <Button
                variant={selecting ? "subtle" : "outline"}
                size="sm"
                onClick={toggleSelecting}
                aria-pressed={selecting}
                title={t("selectSessionsHint")}
              >
                <ListChecks size={13} />
                {t(selecting ? "exitSelect" : "selectSessions")}
              </Button>
            )}
          </div>
        )}
      </div>

      {tab === "prompts" ? (
        promptsQ.isLoading ? (
          <ListSkeleton />
        ) : promptsQ.isError ? (
          <CenterMessage
            icon={<Folder size={28} />}
            title={t("loadFailed")}
            hint={errMessage(promptsQ.error)}
          />
        ) : promptItems.length > 0 ? (
          <PromptList
            items={promptItems}
            showAgentBadge={projectAgentFilter === "all"}
          />
        ) : (
          <CenterMessage
            icon={<Folder size={28} />}
            title={t("noPromptsInFolder")}
            hint={includeCommands ? undefined : t("noPromptsInFolderHint")}
          />
        )
      ) : sessionsQ.isLoading ? (
        <ListSkeleton />
      ) : sessionsQ.isError ? (
        <CenterMessage
          icon={<MessagesSquare size={28} />}
          title={t("loadFailed")}
          hint={errMessage(sessionsQ.error)}
        />
      ) : sessions.length > 0 ? (
        <>
          {selecting && (
            <SessionsExportBar
              project={projectPath}
              sessions={sessions}
              selected={selected}
              onSelectAll={() => setSelected(new Set(sessions.map(sessionKey)))}
              onClear={() => setSelected(new Set())}
            />
          )}
          <div className="space-y-2.5">
            {sessions.map((s) => {
              const key = sessionKey(s);
              return (
                <SessionRow
                  key={key}
                  session={s}
                  showAgentBadge={projectAgentFilter === "all"}
                  selectable={selecting}
                  selected={selected.has(key)}
                  onToggle={() => toggleSelected(key)}
                />
              );
            })}
          </div>
        </>
      ) : (
        <CenterMessage
          icon={<MessagesSquare size={28} />}
          title={t("noConversationsInFolder")}
          hint={t("noConversationsInFolderHint")}
        />
      )}
    </div>
  );
}
