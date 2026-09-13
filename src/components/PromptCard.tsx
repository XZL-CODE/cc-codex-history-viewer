import { useState } from "react";
import { Link } from "react-router-dom";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Clipboard,
  Command,
  Copy,
  Folder,
  GitBranch,
  MessageSquare,
} from "lucide-react";
import type { PromptEntry, PromptOrigin } from "@/lib/types";
import { useCopy } from "@/hooks/useCopy";
import { useStore } from "@/store";
import { useT, type DictKey } from "@/i18n";
import {
  absoluteTime,
  cn,
  encodePath,
  formatNumber,
  prettyPath,
  relativeTime,
} from "@/lib/utils";
import { Highlight } from "./Highlight";
import { Badge } from "./ui";
import { AgentBadge } from "./AgentBadge";

const originLabelKey: Record<PromptOrigin, DictKey> = {
  history: "originHistory",
  conversation: "originConversation",
  both: "originBoth",
};

export function PromptCard({
  entry,
  ranges,
  showProject = false,
  showAgentBadge = true,
}: {
  entry: PromptEntry;
  ranges?: [number, number][];
  showProject?: boolean;
  showAgentBadge?: boolean;
}) {
  const t = useT();
  const { setProjectAgentFilter } = useStore();
  const [expanded, setExpanded] = useState(false);
  const { copied, copy } = useCopy();
  const collapsible = entry.charCount > 150 || entry.text.includes("\n");
  const originKey = originLabelKey[entry.origin];

  return (
    <article
      className="rounded-lg border border-border bg-surface p-3.5 transition-[border-color,box-shadow] hover:border-accent/40 hover:shadow-sm"
      data-prompt-card
      data-agent={entry.agent}
    >
      <div
        onClick={() => collapsible && setExpanded((v) => !v)}
        className={cn(
          "whitespace-pre-wrap break-words text-sm leading-relaxed text-foreground",
          collapsible && "cursor-pointer",
          !expanded && "line-clamp-3"
        )}
      >
        <Highlight text={entry.text} ranges={ranges} />
      </div>

      {collapsible && (
        <button
          onClick={() => setExpanded((v) => !v)}
          className="mt-1 flex items-center gap-0.5 text-[11px] font-medium text-accent hover:underline"
        >
          {expanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          {expanded ? t("collapse") : t("expandFull")}
        </button>
      )}

      <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[11px] text-muted">
        <span title={absoluteTime(entry.timestamp)}>
          {relativeTime(entry.timestamp)}
        </span>

        {showProject && entry.project && (
          <Link
            to={`/project/${encodePath(entry.project)}`}
            onClick={() => setProjectAgentFilter("all")}
            className="flex items-center gap-1 transition-colors hover:text-accent"
            title={entry.project}
          >
            <Folder size={11} />
            <span className="max-w-[220px] truncate">
              {prettyPath(entry.project)}
            </span>
          </Link>
        )}

        {entry.isCommand && (
          <Badge tone="warning">
            <Command size={10} />
            {t("commandBadge")}
          </Badge>
        )}

        {showAgentBadge && <AgentBadge agent={entry.agent} />}

        <Badge tone="muted">{t(originKey)}</Badge>

        {entry.gitBranch && (
          <span className="flex items-center gap-1">
            <GitBranch size={11} />
            {entry.gitBranch}
          </span>
        )}

        {entry.pastedCount > 0 && (
          <span className="flex items-center gap-1">
            <Clipboard size={11} />
            {t("pastedCount", { count: entry.pastedCount })}
          </span>
        )}

        <span className="ml-auto flex items-center justify-end gap-2">
          <span>{t("charCount", { count: formatNumber(entry.charCount) })}</span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              copy(entry.text);
            }}
            title={t("copyPrompt")}
            className={cn(
              "flex h-7 w-7 items-center justify-center rounded-md transition-colors hover:bg-surface-2",
              copied ? "text-success" : "text-muted hover:text-accent"
            )}
          >
            {copied ? <Check size={12} /> : <Copy size={12} />}
          </button>
          {entry.sessionId && entry.hasConversation && (
            <Link
              to={`/conversation/${entry.agent}/${entry.sessionId}?t=${entry.timestamp}`}
              className="flex items-center gap-1 font-medium text-accent hover:underline"
            >
              <MessageSquare size={11} />
              {t("viewConversation")}
            </Link>
          )}
        </span>
      </div>
    </article>
  );
}
