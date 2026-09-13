import { useMemo, type ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  CircleDollarSign,
  DatabaseZap,
  Sigma,
} from "lucide-react";
import type { AgentFilter, DayUsage, UsageStats } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui";
import { AgentBadge } from "@/components/AgentBadge";
import { useT } from "@/i18n";
import {
  cn,
  encodePath,
  formatCost,
  formatNumber,
  formatTokens,
  formatUsageCost,
  usageTotal as totalTokens,
} from "@/lib/utils";

const AXIS = "var(--muted)";
const GRID = "var(--border)";
const ACCENT = "var(--accent)";

function shortModel(model: string): string {
  return model.replace(/^claude-/, "");
}

function StatCard({
  icon,
  label,
  value,
  sub,
  prominent = false,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  sub?: string;
  prominent?: boolean;
}) {
  return (
    <div
      className={
        prominent
          ? "rounded-lg border border-accent/50 bg-accent/5 p-4"
          : "rounded-lg border border-border bg-surface p-4"
      }
    >
      <div className="flex items-center gap-1.5 text-xs text-muted">
        {icon}
        <span>{label}</span>
      </div>
      <div className="mt-1.5 text-2xl font-semibold text-foreground">
        {value}
      </div>
      {sub && <div className="mt-0.5 text-[11px] text-muted">{sub}</div>}
    </div>
  );
}

interface DayTooltipProps {
  active?: boolean;
  payload?: { payload?: DayUsage }[];
  label?: string | number;
}

function DayUsageTooltip({ active, payload, label }: DayTooltipProps) {
  const t = useT();
  const row = payload?.[0]?.payload;
  if (!active || !row) return null;
  return (
    <div className="max-w-[260px] rounded-lg border border-border bg-surface px-2.5 py-2 text-xs shadow-lg">
      <div className="font-medium text-foreground">{label}</div>
      <div className="mt-1 space-y-0.5 text-muted">
        <div className="font-medium text-foreground">
          {t("totalTokensLine", { value: formatTokens(totalTokens(row)) })}
        </div>
        <div>
          {t("inputOutputLine", {
            input: formatTokens(row.uncachedInput),
            output: formatTokens(row.output),
          })}
        </div>
        <div>{t("cacheReadLine", { value: formatTokens(row.cacheRead) })}</div>
        <div>
          {t("cacheCreationLine", { value: formatTokens(row.cacheCreation) })}
        </div>
        <div>
          {t("reasoningOutputLine", {
            value: formatTokens(row.reasoningOutput),
          })}
        </div>
        <div>
          {t("costLabel")} {formatUsageCost(row)}
        </div>
        {row.unknownModelTokens > 0 && (
          <div className="text-warning">
            {t("unknownModelNote", {
              value: formatTokens(row.unknownModelTokens),
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function DayUsageChart({ data }: { data: DayUsage[] }) {
  const t = useT();
  const rows = useMemo(
    () =>
      data.map((row) => ({
        ...row,
        totalTokensIncludingCache: totalTokens(row),
      })),
    [data]
  );
  if (rows.length === 0) {
    return (
      <div className="flex h-52 items-center justify-center text-xs text-muted">
        {t("noData")}
      </div>
    );
  }
  return (
    <ResponsiveContainer width="100%" height={208}>
      <BarChart data={rows} margin={{ top: 8, right: 8, bottom: 0, left: -2 }}>
        <CartesianGrid stroke={GRID} strokeDasharray="3 3" vertical={false} />
        <XAxis
          dataKey="day"
          tick={{ fill: AXIS, fontSize: 10 }}
          tickFormatter={(day: string) => day.slice(5)}
          interval="preserveStartEnd"
          minTickGap={28}
          stroke={GRID}
        />
        <YAxis
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
          width={50}
          tickFormatter={(value: number) => formatTokens(value)}
        />
        <Tooltip
          content={<DayUsageTooltip />}
          cursor={{ fill: "var(--surface-2)" }}
        />
        <Bar
          dataKey="totalTokensIncludingCache"
          fill={ACCENT}
          radius={[3, 3, 0, 0]}
        />
      </BarChart>
    </ResponsiveContainer>
  );
}

function ModelTable({
  usage,
  showAgentBadges,
}: {
  usage: UsageStats;
  showAgentBadges: boolean;
}) {
  const t = useT();
  if (usage.byModel.length === 0) {
    return <div className="py-8 text-center text-xs text-muted">{t("noData")}</div>;
  }
  return (
    <div className="overflow-x-auto">
      <table
        className={cn(
          "w-full text-xs",
          showAgentBadges ? "min-w-[900px]" : "min-w-[820px]"
        )}
      >
        <thead>
          <tr className="border-b border-border text-muted">
            {showAgentBadges && (
              <th className="pb-2 pr-2 text-left font-medium">
                {t("agentCol")}
              </th>
            )}
            <th className="px-2 pb-2 text-left font-medium">{t("modelCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("messagesCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("totalTokensCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("inputCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("cacheReadCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("cacheCreationCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("outputCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("reasoningCol")}</th>
            <th className="pb-2 pl-2 text-right font-medium">{t("estCostCol")}</th>
          </tr>
        </thead>
        <tbody>
          {usage.byModel.map((row) => (
            <tr
              key={`${row.agent}:${row.model}`}
              className="border-b border-border/60 last:border-0"
            >
              {showAgentBadges && (
                <td className="py-2 pr-2">
                  <AgentBadge agent={row.agent} />
                </td>
              )}
              <td className="px-2 py-2">
                <span
                  className="block max-w-[190px] truncate font-medium text-foreground"
                  title={row.model}
                >
                  {shortModel(row.model)}
                </span>
              </td>
              <td className="px-2 py-2 text-right text-muted">{formatNumber(row.messages)}</td>
              <td className="px-2 py-2 text-right font-medium text-foreground">
                {formatTokens(totalTokens(row))}
              </td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.uncachedInput)}</td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.cacheRead)}</td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.cacheCreation)}</td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.output)}</td>
              <td className="px-2 py-2 text-right text-muted" title={t("reasoningOutputLine", { value: formatTokens(row.reasoningOutput) })}>
                {formatTokens(row.reasoningOutput)}
              </td>
              <td className="py-2 pl-2 text-right font-medium text-foreground">
                {formatCost(row.estCostUsd)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ProjectUsageList({
  usage,
  showAgentBadges,
}: {
  usage: UsageStats;
  showAgentBadges: boolean;
}) {
  const t = useT();
  const top = useMemo(
    () =>
      [...usage.byProject]
        .sort((a, b) => totalTokens(b) - totalTokens(a))
        .slice(0, 8),
    [usage.byProject]
  );
  if (top.length === 0) {
    return <div className="py-8 text-center text-xs text-muted">{t("noData")}</div>;
  }
  const maxTotal = Math.max(...top.map(totalTokens), 1);
  return (
    <div className="space-y-2.5">
      {top.map((row) => (
        <Link
          key={row.path}
          to={`/project/${encodePath(row.path)}`}
          className="block rounded-lg px-2 py-1.5 transition-colors hover:bg-surface-2"
          title={row.path}
        >
          <div className="flex min-w-0 items-start justify-between gap-3 text-xs">
            <div className="min-w-0">
              <div className="truncate font-medium text-foreground">{row.name}</div>
              {showAgentBadges && (
                <div className="mt-1 flex flex-wrap gap-1">
                  {row.agents.map((agent) => (
                    <AgentBadge key={agent} agent={agent} />
                  ))}
                </div>
              )}
            </div>
            <span className="shrink-0 text-right text-muted">
              <span className="block">{t("tokenTotalSuffix", { value: formatTokens(totalTokens(row)) })}</span>
              <span
                className="block"
                title={
                  row.unknownModelTokens > 0
                    ? t("unknownModelNote", {
                        value: formatTokens(row.unknownModelTokens),
                      })
                    : undefined
                }
              >
                {formatUsageCost(row)}
              </span>
            </span>
          </div>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full bg-accent"
              style={{ width: `${Math.max((totalTokens(row) / maxTotal) * 100, 2)}%` }}
            />
          </div>
        </Link>
      ))}
    </div>
  );
}

export function TokenStats({
  usage,
  agentFilter,
}: {
  usage: UsageStats;
  agentFilter: AgentFilter;
}) {
  const t = useT();
  const total = totalTokens(usage);
  if (total === 0) {
    return (
      <section>
        <h2 className="mb-3 text-sm font-semibold text-foreground">{t("tokenUsageTitle")}</h2>
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted">{t("noTokenData")}</CardContent>
        </Card>
      </section>
    );
  }

  const cacheableInput = usage.uncachedInput + usage.cacheRead;
  const input = cacheableInput + usage.cacheCreation;
  const hitRate = cacheableInput > 0
    ? `${((usage.cacheRead / cacheableInput) * 100).toFixed(1)}%`
    : "—";
  const costLabel =
    agentFilter === "codex"
      ? t("apiEquivalentCost")
      : agentFilter === "all"
        ? t("knownPriceCost")
        : t("estTotalCost");
  const costNote =
    agentFilter === "codex"
      ? t("costNoteCodex")
      : agentFilter === "all"
        ? t("costNoteAll")
        : t("costNoteClaude");

  return (
    <section className="min-w-0 space-y-3">
      <h2 className="text-sm font-semibold text-foreground">{t("tokenUsageTitle")}</h2>
      <div className="grid grid-cols-1 gap-3 min-[420px]:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
        <StatCard
          prominent
          icon={<Sigma size={13} />}
          label={t("totalTokensCard")}
          value={formatTokens(total)}
          sub={t("totalTokensSub")}
        />
        <StatCard
          icon={<CircleDollarSign size={13} />}
          label={costLabel}
          value={formatUsageCost(usage)}
          sub={t("assistantMessagesSub", { count: formatNumber(usage.assistantMessages) })}
        />
        <StatCard
          icon={<ArrowUpFromLine size={13} />}
          label={t("outputTokensCard")}
          value={formatTokens(usage.output)}
          sub={t("outputTokensSub", { value: formatTokens(usage.reasoningOutput) })}
        />
        <StatCard
          icon={<ArrowDownToLine size={13} />}
          label={t("inputTokensCard")}
          value={formatTokens(input)}
          sub={t("inputTokensSub", {
            uncached: formatTokens(usage.uncachedInput),
            cached: formatTokens(usage.cacheRead),
            created: formatTokens(usage.cacheCreation),
          })}
        />
        <StatCard
          icon={<DatabaseZap size={13} />}
          label={t("cacheHitRate")}
          value={hitRate}
          sub={t("cacheHitRateSub")}
        />
      </div>

      <div className="space-y-0.5">
        <p className="text-[11px] text-muted">{costNote}</p>
        {usage.unknownModelTokens > 0 && (
          <p className="text-[11px] text-warning">
            {t("unknownModelNote", { value: formatTokens(usage.unknownModelTokens) })}
          </p>
        )}
      </div>

      <Card className="min-w-0">
        <CardHeader><CardTitle>{t("dailyCost")}</CardTitle></CardHeader>
        <CardContent><DayUsageChart data={usage.byDay} /></CardContent>
      </Card>

      <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-2">
        <Card className="min-w-0">
          <CardHeader><CardTitle>{t("byModel")}</CardTitle></CardHeader>
          <CardContent>
            <ModelTable
              usage={usage}
              showAgentBadges={agentFilter === "all"}
            />
          </CardContent>
        </Card>
        <Card className="min-w-0">
          <CardHeader><CardTitle>{t("topCostFolders")}</CardTitle></CardHeader>
          <CardContent>
            <ProjectUsageList
              usage={usage}
              showAgentBadges={agentFilter === "all"}
            />
          </CardContent>
        </Card>
      </div>
    </section>
  );
}
