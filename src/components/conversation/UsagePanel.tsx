import { useEffect, useMemo, useRef, useState } from "react";
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
  ChevronRight,
  CircleDollarSign,
  DatabaseZap,
  Sigma,
} from "lucide-react";
import type {
  Agent,
  CostParts,
  SessionModelUsage,
  SessionUsage,
  UsageBreakdown,
} from "@/lib/types";
import { useT, type DictKey } from "@/i18n";
import { StatCard } from "@/components/StatCard";
import {
  absoluteTime,
  cacheHitRate,
  cn,
  formatCost,
  formatNumber,
  formatPercent,
  formatTokens,
  formatUsageCost,
  usageContext,
  usageTotal,
} from "@/lib/utils";

const OPEN_KEY = "cchv-usage-panel";
const AXIS = "var(--muted)";
const GRID = "var(--border)";

/** Token 四个类目的固定序列色与文案：构成条与调用图共用同一顺序，颜色跟随类目而不是大小 */
const SERIES: { key: keyof CostParts; labelKey: DictKey; color: string }[] = [
  { key: "cacheRead", labelKey: "usageSeriesCacheRead", color: "var(--series-1)" },
  { key: "cacheCreation", labelKey: "usageSeriesCacheWrite", color: "var(--series-2)" },
  { key: "uncachedInput", labelKey: "usageSeriesUncached", color: "var(--series-3)" },
  { key: "output", labelKey: "usageSeriesOutput", color: "var(--series-4)" },
];

function readOpen(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === "open";
  } catch {
    return false;
  }
}

function shortModel(model: string): string {
  return model.replace(/^claude-/, "");
}

function SeriesLegend({
  values,
  className,
}: {
  /** 与 SERIES 顺序对应的数值文本（如百分比） */
  values?: string[];
  className?: string;
}) {
  const t = useT();
  return (
    <ul className={cn("flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted", className)}>
      {SERIES.map((series, index) => (
        <li key={series.key} className="flex items-center gap-1.5">
          <span
            aria-hidden
            className="h-2 w-2 shrink-0 rounded-sm"
            style={{ background: series.color }}
          />
          <span>{t(series.labelKey)}</span>
          {values?.[index] && (
            <span className="tabular-nums text-foreground">{values[index]}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

/** 一根 100% 堆叠条：段之间留 2px 底色缝隙，右侧图例带各段占比 */
function CompositionBar({ label, parts }: { label: string; parts: CostParts }) {
  const t = useT();
  const total = SERIES.reduce((sum, series) => sum + parts[series.key], 0);
  if (total <= 0) return null;
  const shares = SERIES.map((series) => parts[series.key] / total);
  const legend = SERIES.map(
    (series, index) => `${t(series.labelKey)} ${formatPercent(shares[index])}`
  ).join(" · ");
  return (
    <div className="flex flex-col gap-1.5 sm:flex-row sm:items-center sm:gap-3">
      <span className="w-20 shrink-0 text-[11px] text-muted">{label}</span>
      <div
        className="flex h-2.5 w-full min-w-0 gap-[2px] overflow-hidden rounded-full sm:w-auto sm:flex-1"
        role="img"
        aria-label={`${label}: ${legend}`}
      >
        {SERIES.map((series, index) =>
          shares[index] > 0 ? (
            <div
              key={series.key}
              className="h-full rounded-sm"
              style={{
                flexGrow: shares[index],
                flexBasis: 0,
                minWidth: 3,
                background: series.color,
              }}
            />
          ) : null
        )}
      </div>
      <SeriesLegend
        className="sm:w-[22rem] sm:shrink-0"
        values={shares.map((share) =>
          formatPercent(share, share > 0 && share < 0.01 ? 2 : 1)
        )}
      />
    </div>
  );
}

interface CallRow {
  n: number;
  callKey: string;
  model: string;
  timestamp: number;
  uncachedInput: number;
  cacheRead: number;
  cacheCreation: number;
  cacheCreation1h: number;
  output: number;
  reasoningOutput: number;
  estCostUsd: number | null;
}

function CallTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: { payload?: CallRow }[];
}) {
  const t = useT();
  const row = payload?.[0]?.payload;
  if (!active || !row) return null;
  return (
    <div className="max-w-[300px] rounded-lg border border-border bg-surface px-2.5 py-2 text-xs shadow-lg">
      <div className="font-medium text-foreground">
        {t("usageCallLabel", { n: row.n })} · {shortModel(row.model)}
      </div>
      <div className="text-muted">{absoluteTime(row.timestamp)}</div>
      <div className="mt-1 space-y-0.5 text-muted">
        <div className="text-foreground">
          {t("usageContextLine", { value: formatTokens(usageContext(row)) })} ·{" "}
          {t("usageOutputLine", { value: formatTokens(row.output) })}
        </div>
        <div>
          {t("usageCacheReadLine", { value: formatTokens(row.cacheRead) })} ·{" "}
          {t("usageCacheWriteLine", { value: formatTokens(row.cacheCreation) })} ·{" "}
          {t("usageUncachedLine", { value: formatTokens(row.uncachedInput) })}
        </div>
        {row.reasoningOutput > 0 && (
          <div>
            {t("reasoningOutputShort", { value: formatTokens(row.reasoningOutput) })}
          </div>
        )}
        <div>
          {t("usageCallCostLine", {
            cost: row.estCostUsd === null ? "—" : formatCost(row.estCostUsd),
            hit: formatPercent(cacheHitRate(row)),
          })}
        </div>
      </div>
      <div className="mt-1 text-[10px] text-muted">{t("usageClickToJump")}</div>
    </div>
  );
}

/** 每次调用一根堆叠柱，四段同一单位（tokens），只有一条 y 轴；点击整根柱子跳到对应消息 */
function CallChart({
  rows,
  onPick,
}: {
  rows: CallRow[];
  onPick: (callKey: string) => void;
}) {
  // 段与段之间用 1px 底色描边充当缝隙；柱子窄于 5px 时缝隙会把柱子吃掉，此时不描边
  const wrapperRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const node = wrapperRef.current;
    if (!node) return;
    const observer = new ResizeObserver((entries) => {
      setWidth(entries[0]?.contentRect.width ?? 0);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const gap = width > 0 && (width - 64) / rows.length >= 5 ? 1 : 0;
  return (
    <div ref={wrapperRef}>
    <ResponsiveContainer width="100%" height={220}>
      <BarChart
        data={rows}
        margin={{ top: 8, right: 8, bottom: 0, left: -2 }}
        className="cursor-pointer"
        onClick={(state) => {
          const index = state?.activeTooltipIndex;
          if (typeof index === "number" && rows[index]) onPick(rows[index].callKey);
        }}
      >
        <CartesianGrid stroke={GRID} vertical={false} />
        <XAxis
          dataKey="n"
          tick={{ fill: AXIS, fontSize: 10 }}
          interval="preserveStartEnd"
          minTickGap={24}
          stroke={GRID}
          tickLine={false}
        />
        <YAxis
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
          tickLine={false}
          width={50}
          tickFormatter={(value: number) => formatTokens(value)}
        />
        <Tooltip content={<CallTooltip />} cursor={{ fill: "var(--surface-2)" }} />
        {SERIES.map((series, index) => (
          <Bar
            key={series.key}
            dataKey={series.key}
            stackId="call"
            fill={series.color}
            stroke={gap ? "var(--surface)" : undefined}
            strokeWidth={gap}
            maxBarSize={24}
            isAnimationActive={false}
            radius={index === SERIES.length - 1 ? [3, 3, 0, 0] : undefined}
          />
        ))}
      </BarChart>
    </ResponsiveContainer>
    </div>
  );
}

function ModelTable({ rows }: { rows: SessionModelUsage[] }) {
  const t = useT();
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[640px] text-xs tabular-nums">
        <thead>
          <tr className="border-b border-border text-muted">
            <th className="pb-2 pr-2 text-left font-medium">{t("modelCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("callsCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("totalTokensCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("inputCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("cacheReadCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("cacheCreationCol")}</th>
            <th className="px-2 pb-2 text-right font-medium">{t("outputCol")}</th>
            <th className="pb-2 pl-2 text-right font-medium">{t("estCostCol")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.model} className="border-b border-border/60 last:border-0">
              <td className="py-2 pr-2">
                <span
                  className="block max-w-[190px] truncate font-medium text-foreground"
                  title={row.model}
                >
                  {shortModel(row.model)}
                </span>
              </td>
              <td className="px-2 py-2 text-right text-muted">{formatNumber(row.calls)}</td>
              <td className="px-2 py-2 text-right font-medium text-foreground">
                {formatTokens(row.totalTokensIncludingCache)}
              </td>
              <td className="px-2 py-2 text-right text-muted">
                {formatTokens(row.uncachedInput)}
              </td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.cacheRead)}</td>
              <td className="px-2 py-2 text-right text-muted">
                {formatTokens(row.cacheCreation)}
              </td>
              <td className="px-2 py-2 text-right text-muted">{formatTokens(row.output)}</td>
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

/**
 * 会话用量明细：折叠面板，收起时只剩一行摘要。展开后是 5 张指标卡（均值 / 占比 / 缓存节省 /
 * 缓存分档放在卡片第二行）、Token 与成本两根构成条、按调用顺序的堆叠柱图，多模型会话再加一张
 * 按模型表。所有数字与页头的会话总量同源（索引归属到本会话的调用）。
 */
export function UsagePanel({
  agent,
  usage,
  breakdown,
  turns,
  callIndexByKey,
  onJump,
}: {
  agent: Agent;
  usage: SessionUsage;
  breakdown: UsageBreakdown;
  /** 用户轮次数，用于每轮平均成本 */
  turns: number;
  /** 调用指纹 → 消息下标，用于从图表跳到消息 */
  callIndexByKey: Map<string, number>;
  onJump: (index: number) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(readOpen);
  const toggle = () =>
    setOpen((value) => {
      const next = !value;
      try {
        localStorage.setItem(OPEN_KEY, next ? "open" : "closed");
      } catch {
        /* 忽略持久化失败 */
      }
      return next;
    });

  const subagentTokens = useMemo(
    () =>
      breakdown.calls
        .filter((call) => call.subagent)
        .reduce((sum, call) => sum + usageTotal(call), 0),
    [breakdown.calls]
  );
  const mainRows = useMemo<CallRow[]>(
    () =>
      breakdown.calls
        .filter((call) => !call.subagent)
        .map((call, index) => ({
          n: index + 1,
          callKey: call.callKey,
          model: call.model,
          timestamp: call.timestamp,
          uncachedInput: call.uncachedInput,
          cacheRead: call.cacheRead,
          cacheCreation: call.cacheCreation,
          cacheCreation1h: call.cacheCreation1h,
          output: call.output,
          reasoningOutput: call.reasoningOutput,
          estCostUsd: call.estCostUsd,
        })),
    [breakdown.calls]
  );

  const total = usage.totalTokensIncludingCache;
  if (total <= 0) return null;

  const calls = usage.assistantMessages;
  const context = usageContext(usage);
  const priced = usage.unknownModelTokens < total;
  const savings = breakdown.estCostNoCacheUsd - usage.estCostUsd;
  const savingsRatio =
    breakdown.estCostNoCacheUsd > 0 ? savings / breakdown.estCostNoCacheUsd : null;
  const costTotal =
    breakdown.costParts.uncachedInput +
    breakdown.costParts.cacheRead +
    breakdown.costParts.cacheCreation +
    breakdown.costParts.output;
  const pick = (callKey: string) => {
    const index = callIndexByKey.get(callKey);
    if (index !== undefined) onJump(index);
  };

  return (
    <section className="mt-3 rounded-lg border border-border bg-surface">
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-surface-2/60"
      >
        <ChevronRight
          size={14}
          className={cn("shrink-0 text-muted transition-transform", open && "rotate-90")}
        />
        <Sigma size={13} className="shrink-0 text-muted" />
        <span className="shrink-0 whitespace-nowrap text-xs font-semibold text-foreground">
          {t("usagePanelTitle")}
        </span>
        <span
          className="ml-auto min-w-0 truncate text-[11px] tabular-nums text-muted"
          title={t("tokenTotalSuffix", { value: formatNumber(total) })}
        >
          {t("usagePanelSummary", {
            tokens: formatTokens(total),
            cost: formatUsageCost(usage),
            calls: formatNumber(calls),
          })}
        </span>
      </button>

      {open && (
        <div className="space-y-4 border-t border-border px-3 py-3">
          <div className="grid grid-cols-1 gap-3 min-[420px]:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
            <StatCard
              prominent
              icon={<Sigma size={13} />}
              label={t("totalTokensCard")}
              value={formatTokens(total)}
              sub={
                <>
                  <span className="block">
                    {t("usageCallsCount", { count: formatNumber(calls) })}
                  </span>
                  {calls > 0 && (
                    <span className="block">
                      {t("usageAvgContextPerCall", {
                        value: formatTokens(Math.round(context / calls)),
                      })}
                    </span>
                  )}
                </>
              }
            />
            <StatCard
              icon={<CircleDollarSign size={13} />}
              label={agent === "codex" ? t("apiEquivalentCost") : t("estTotalCost")}
              value={formatUsageCost(usage)}
              sub={
                <>
                  {priced && turns > 0 && (
                    <span className="block">
                      {t("usageCostPerTurn", {
                        value: formatCost(usage.estCostUsd / turns),
                      })}
                    </span>
                  )}
                  {subagentTokens > 0 && (
                    <span className="block">
                      {t("usageSubagentShare", {
                        value: formatPercent(subagentTokens / total),
                      })}
                    </span>
                  )}
                </>
              }
            />
            <StatCard
              icon={<ArrowDownToLine size={13} />}
              label={t("inputTokensCard")}
              value={formatTokens(context)}
              sub={
                <>
                  <span className="block">
                    {t("usageUncachedLine", { value: formatTokens(usage.uncachedInput) })} ·{" "}
                    {t("usageCacheReadLine", { value: formatTokens(usage.cacheRead) })}
                  </span>
                  {usage.cacheCreation1h > 0 ? (
                    <span className="block">
                      {t("usageCacheWriteTiersLine", {
                        m5: formatTokens(usage.cacheCreation - usage.cacheCreation1h),
                        h1: formatTokens(usage.cacheCreation1h),
                      })}
                    </span>
                  ) : (
                    <span className="block">
                      {t("usageCacheWriteLine", { value: formatTokens(usage.cacheCreation) })}
                    </span>
                  )}
                </>
              }
            />
            <StatCard
              icon={<ArrowUpFromLine size={13} />}
              label={t("outputTokensCard")}
              value={formatTokens(usage.output)}
              sub={
                <>
                  {usage.reasoningOutput > 0 && usage.output > 0 && (
                    <span className="block">
                      {t("usageReasoningShare", {
                        value: formatPercent(usage.reasoningOutput / usage.output),
                      })}
                    </span>
                  )}
                  {calls > 0 && (
                    <span className="block">
                      {t("usageAvgOutputPerCall", {
                        value: formatTokens(Math.round(usage.output / calls)),
                      })}
                    </span>
                  )}
                </>
              }
            />
            <StatCard
              icon={<DatabaseZap size={13} />}
              label={t("cacheHitRate")}
              value={formatPercent(cacheHitRate(usage))}
              sub={
                breakdown.estCostNoCacheUsd > 0 ? (
                  <>
                    <span className="block">
                      {savings >= 0
                        ? t("usageCacheSavings", { value: formatCost(savings) })
                        : t("usageCacheOverhead", { value: formatCost(-savings) })}
                    </span>
                    <span className="block">
                      {savings >= 0
                        ? t("usageVsNoCacheSaved", { value: formatPercent(savingsRatio) })
                        : t("usageVsNoCacheExtra", {
                            value: formatPercent(
                              savingsRatio === null ? null : -savingsRatio
                            ),
                          })}
                    </span>
                  </>
                ) : (
                  t("cacheHitRateSub")
                )
              }
            />
          </div>

          <div className="space-y-2">
            <CompositionBar
              label={t("usageTokenComposition")}
              parts={{
                uncachedInput: usage.uncachedInput,
                cacheRead: usage.cacheRead,
                cacheCreation: usage.cacheCreation,
                output: usage.output,
              }}
            />
            {costTotal > 0 && (
              <CompositionBar label={t("usageCostComposition")} parts={breakdown.costParts} />
            )}
          </div>

          {mainRows.length >= 2 && (
            <div>
              <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <span className="text-xs font-semibold text-foreground">
                  {t("usageTimelineTitle")}
                </span>
                <span className="text-[11px] text-muted">{t("usageTimelineHint")}</span>
              </div>
              <div className="mt-2">
                <CallChart rows={mainRows} onPick={pick} />
              </div>
              <SeriesLegend className="mt-1 justify-center" />
            </div>
          )}

          {breakdown.byModel.length > 1 && (
            <div>
              <div className="mb-2 text-xs font-semibold text-foreground">
                {t("usageByModelTitle")}
              </div>
              <ModelTable rows={breakdown.byModel} />
            </div>
          )}

          <div className="space-y-0.5 text-[11px] text-muted">
            <p>{agent === "codex" ? t("costNoteCodex") : t("costNoteClaude")}</p>
            {usage.unknownModelTokens > 0 && (
              <p className="text-warning">
                {t("unknownModelNote", { value: formatTokens(usage.unknownModelTokens) })}
              </p>
            )}
            {breakdown.unattributedCalls > 0 && (
              <p>
                {t("usageUnattributedNote", {
                  count: formatNumber(breakdown.unattributedCalls),
                })}
              </p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
