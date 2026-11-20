import { Layers3 } from "lucide-react";
import type { Agent, AgentFilter } from "@/lib/types";
import { useT, type DictKey } from "@/i18n";
import { AGENTS, cn } from "@/lib/utils";
import { Badge } from "./ui";

const AGENT_LABEL: Record<Agent, DictKey> = {
  claude: "agentClaude",
  codex: "agentCodex",
  antigravity: "agentAntigravity",
};
const AGENT_TONE = {
  claude: "warning",
  codex: "success",
  antigravity: "info",
} as const satisfies Record<Agent, string>;
const AGENT_DOT: Record<Agent, string> = {
  claude: "bg-warning",
  codex: "bg-success",
  antigravity: "bg-info",
};

export function AgentBadge({
  agent,
  className,
}: {
  agent: Agent;
  className?: string;
}) {
  const t = useT();
  return (
    <Badge tone={AGENT_TONE[agent]} className={className} data-agent-badge={agent}>
      <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden />
      {t(AGENT_LABEL[agent])}
    </Badge>
  );
}

const OPTIONS: AgentFilter[] = [...AGENTS, "all"];

export function AgentFilterControl({
  value,
  onChange,
  className,
  compact = false,
  ariaLabel,
}: {
  value: AgentFilter;
  onChange: (value: AgentFilter) => void;
  className?: string;
  compact?: boolean;
  ariaLabel?: string;
}) {
  const t = useT();
  return (
    <div
      className={cn(
        "inline-flex max-w-full items-center rounded-lg border border-border bg-background p-0.5",
        className
      )}
      role="group"
      aria-label={ariaLabel ?? t("agentFilterLabel")}
      data-agent-filter={ariaLabel ?? t("agentFilterLabel")}
    >
      {OPTIONS.map((option) => {
        const fullLabel = option === "all" ? t("agentAll") : t(AGENT_LABEL[option]);
        const label =
          compact && option === "claude" ? t("agentClaudeShort") : fullLabel;

        return (
          <button
            key={option}
            type="button"
            aria-pressed={value === option}
            onClick={() => onChange(option)}
            title={compact ? fullLabel : undefined}
            className={cn(
              "flex h-8 min-w-0 items-center justify-center gap-1.5 rounded-md px-3 text-xs font-medium transition-colors",
              value === option
                ? "bg-accent text-accent-fg"
                : "text-muted hover:text-foreground"
            )}
          >
            {option === "all" ? (
              <Layers3 size={13} aria-hidden />
            ) : (
              <span
                className={cn("h-1.5 w-1.5 shrink-0 rounded-full", AGENT_DOT[option])}
                aria-hidden
              />
            )}
            <span className="truncate">{label}</span>
          </button>
        );
      })}
    </div>
  );
}
