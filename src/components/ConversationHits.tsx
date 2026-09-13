// 全文搜索的命中列表：按会话分组，每条片段可跳转到对话中的具体消息。

import { useMemo } from "react";
import { Link } from "react-router-dom";
import { Brain, Folder, MessageSquare, Wrench } from "lucide-react";
import type { ConversationHit } from "@/lib/types";
import { useT } from "@/i18n";
import { absoluteTime, cn, encodePath, prettyPath } from "@/lib/utils";
import { AgentBadge } from "./AgentBadge";
import { Highlight } from "./Highlight";
import { Badge } from "./ui";

interface SessionGroup {
  key: string;
  agent: ConversationHit["agent"];
  sessionId: string;
  project: string;
  title: string;
  startedAt: number;
  hits: ConversationHit[];
}

function groupBySession(hits: ConversationHit[]): SessionGroup[] {
  const groups = new Map<string, SessionGroup>();
  for (const hit of hits) {
    const key = `${hit.agent}:${hit.sessionId}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        agent: hit.agent,
        sessionId: hit.sessionId,
        project: hit.project,
        title: hit.sessionTitle,
        startedAt: hit.sessionStartedAt,
        hits: [],
      };
      groups.set(key, group);
    }
    group.hits.push(hit);
  }
  return Array.from(groups.values());
}

function HitRow({
  hit,
  query,
  showAgentBadge,
}: {
  hit: ConversationHit;
  query: string;
  showAgentBadge: boolean;
}) {
  const t = useT();
  const params = new URLSearchParams({
    m: hit.messageUuid,
    t: String(hit.timestamp),
    q: query,
  });
  const kindLabel =
    hit.kind === "thinking"
      ? t("hitKindThinking")
      : hit.kind === "tool_use"
        ? t("hitKindTool", { name: hit.toolName ?? "tool" })
        : null;
  return (
    <Link
      to={`/conversation/${hit.agent}/${hit.sessionId}?${params.toString()}`}
      title={t("openAtMessage")}
      className="block rounded-md px-3 py-2 transition-colors hover:bg-surface-2/70"
    >
      <div className="mb-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted">
        {showAgentBadge && <AgentBadge agent={hit.agent} />}
        <Badge tone={hit.role === "user" ? "accent" : "muted"}>
          {hit.role === "user" ? t("roleUser") : t("hitRoleAssistant")}
        </Badge>
        {kindLabel && (
          <span className="flex items-center gap-1">
            {hit.kind === "thinking" ? <Brain size={11} /> : <Wrench size={11} />}
            {kindLabel}
          </span>
        )}
        <span>{absoluteTime(hit.timestamp)}</span>
      </div>
      <div
        className={cn(
          "break-words text-[13px] leading-relaxed text-foreground",
          hit.kind !== "text" && "font-mono text-[12px] text-muted"
        )}
      >
        <Highlight text={hit.snippet} ranges={hit.matchRanges} />
      </div>
    </Link>
  );
}

export function ConversationHits({
  hits,
  query,
  showProject,
  showAgentBadge,
}: {
  hits: ConversationHit[];
  query: string;
  showProject: boolean;
  showAgentBadge: boolean;
}) {
  const t = useT();
  const groups = useMemo(() => groupBySession(hits), [hits]);
  return (
    <div className="space-y-3">
      {groups.map((group) => (
        <section
          key={group.key}
          className="rounded-lg border border-border bg-surface"
        >
          <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-3.5 py-2.5">
            {showAgentBadge && <AgentBadge agent={group.agent} />}
            <Link
              to={`/conversation/${group.agent}/${group.sessionId}?q=${encodeURIComponent(query)}`}
              className="flex min-w-0 flex-1 items-center gap-1.5 text-sm font-medium text-foreground hover:text-accent"
            >
              <MessageSquare size={13} className="shrink-0 text-muted" />
              <span className="truncate">{group.title || t("untitledSession")}</span>
            </Link>
            <span className="text-[11px] text-muted">
              {t("hitCountInSession", { count: group.hits.length })}
            </span>
            <span className="text-[11px] text-muted">{absoluteTime(group.startedAt)}</span>
            {showProject && group.project && (
              <Link
                to={`/project/${encodePath(group.project)}`}
                className="flex items-center gap-1 text-[11px] text-muted hover:text-accent"
                title={group.project}
              >
                <Folder size={11} />
                <span className="max-w-[220px] truncate">{prettyPath(group.project)}</span>
              </Link>
            )}
          </header>
          <div className="divide-y divide-border/60 py-1">
            {group.hits.map((hit) => (
              <HitRow
                key={`${hit.messageUuid}:${hit.timestamp}:${hit.kind}`}
                hit={hit}
                query={query}
                showAgentBadge={false}
              />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
