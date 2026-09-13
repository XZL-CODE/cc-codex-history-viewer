import { useMemo, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import {
  Clock,
  Folder,
  GitBranch,
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
import { Badge, CenterMessage, Skeleton } from "@/components/ui";
import { useT, type DictKey } from "@/i18n";
import type { SessionSummary, SortMode } from "@/lib/types";
import {
  absoluteTime,
  cn,
  formatDuration,
  formatNumber,
  formatTokens,
  formatUsageCost,
  pathBasename,
} from "@/lib/utils";
import { errMessage } from "@/lib/api";
import { AgentBadge, AgentFilterControl } from "@/components/AgentBadge";

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

function SessionRow({
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
    <Link
      to={`/conversation/${session.agent}/${session.sessionId}`}
      className="block rounded-lg border border-border bg-surface p-3.5 transition-[border-color,box-shadow] hover:border-accent/40 hover:shadow-sm"
    >
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
    </Link>
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
          <SortControl
            options={sessionSortOptions}
            value={sessionSort}
            onChange={setSessionSort}
          />
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
        <div className="space-y-2.5">
          {sessions.map((s) => (
            <SessionRow
              key={`${s.agent}:${s.sessionId}`}
              session={s}
              showAgentBadge={projectAgentFilter === "all"}
            />
          ))}
        </div>
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
