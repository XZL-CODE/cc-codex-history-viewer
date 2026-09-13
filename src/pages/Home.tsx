import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { AlertTriangle, CalendarDays, ListTree, Settings } from "lucide-react";
import { useStore } from "@/store";
import { useIndexMeta, useRecentPrompts, useStats } from "@/hooks/queries";
import { StatsOverview } from "@/components/StatsOverview";
import { HourChart, WeekdayChart } from "@/components/Charts";
import { ActivityHeatmap } from "@/components/Heatmap";
import { RangeControl } from "@/components/RangeControl";
import {
  persistStatsRange,
  readStatsRange,
  resolveRange,
  type StatsRange,
} from "@/lib/statsRange";
import { TokenStats } from "@/components/TokenStats";
import { PromptList } from "@/components/PromptList";
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CenterMessage,
  Skeleton,
} from "@/components/ui";
import { indexProgressLabel } from "@/components/IndexProgress";
import { errMessage } from "@/lib/api";
import { useT } from "@/i18n";
import { absoluteTime, cn, encodePath, formatNumber } from "@/lib/utils";
import { AgentFilterControl } from "@/components/AgentBadge";
import type { ProjectCount } from "@/lib/types";

function StatsSkeleton() {
  return (
    <div className="grid grid-cols-3 gap-3">
      {Array.from({ length: 6 }).map((_, index) => (
        <Skeleton key={index} className="h-24 w-full" />
      ))}
    </div>
  );
}

function ListSkeleton() {
  return (
    <div className="space-y-2.5">
      {Array.from({ length: 5 }).map((_, index) => (
        <Skeleton key={index} className="h-20 w-full" />
      ))}
    </div>
  );
}

function TopProjectsList({ data }: { data: ProjectCount[] }) {
  const t = useT();
  const top = data.slice(0, 4);
  if (top.length === 0) {
    return (
      <div className="flex h-[84px] items-center justify-center text-xs text-muted">
        {t("noData")}
      </div>
    );
  }
  return (
    <div className="grid max-[1200px]:grid-cols-2 max-[1200px]:gap-x-3">
      {top.map((project, index) => (
        <Link
          key={project.path}
          to={`/project/${encodePath(project.path)}`}
          title={project.path}
          className="grid h-[42px] min-w-0 grid-cols-[24px_minmax(0,1fr)_auto] items-center gap-2 border-b border-border px-1.5 text-left last:border-b-0 hover:bg-surface-2/60"
        >
          <span className="flex h-5 w-5 items-center justify-center rounded-md bg-accent/10 text-[11px] font-semibold text-accent">
            {index + 1}
          </span>
          <strong className="truncate text-xs font-medium text-foreground">
            {project.name}
          </strong>
          <span className="text-[11px] text-muted">
            {formatNumber(project.count)}
          </span>
        </Link>
      ))}
    </div>
  );
}

export function Home() {
  const {
    agentFilter,
    includeCommands,
    setAgentFilter,
    indexProgress,
    openSettings,
  } = useStore();
  const t = useT();
  const [range, setRange] = useState<StatsRange>(readStatsRange);
  useEffect(() => {
    persistStatsRange(range);
  }, [range]);
  const resolvedRange = useMemo(() => resolveRange(range), [range]);
  const statsQ = useStats(agentFilter, resolvedRange);
  const metaQ = useIndexMeta();
  const recentQ = useRecentPrompts(24, includeCommands, agentFilter);

  const recentItems = useMemo(
    () => (recentQ.data ?? []).map((entry) => ({ entry })),
    [recentQ.data]
  );

  // 索引元信息行：构建中显示进度，否则显示文件数、构建时间、缓存状态与最新 CLI 版本。
  const metaLine = useMemo(() => {
    if (indexProgress) return indexProgressLabel(indexProgress, t);
    if (!metaQ.data) return t("loadingLocalData");
    const parts = [
      t("indexMetaSummary", {
        files: formatNumber(metaQ.data.sourceFiles),
        time: absoluteTime(metaQ.data.builtAt),
      }),
      metaQ.data.fromCache ? t("indexFromCache") : t("indexFreshScan"),
    ];
    if (metaQ.data.reparsedFiles > 0) {
      parts.push(
        t("indexReparsedFiles", {
          count: formatNumber(metaQ.data.reparsedFiles),
        })
      );
    }
    const versions = statsQ.data?.cliVersions ?? [];
    const claude = versions.find((entry) => entry.agent === "claude");
    const codex = versions.find((entry) => entry.agent === "codex");
    if (claude) parts.push(t("cliVersionClaude", { version: claude.version }));
    if (codex) parts.push(t("cliVersionCodex", { version: codex.version }));
    return parts.join(" · ");
  }, [indexProgress, metaQ.data, statsQ.data?.cliVersions, t]);

  return (
    <div className="page-content space-y-5 py-6">
      <header className="flex items-start justify-between gap-5">
        <div className="min-w-0">
          <h1 className="text-xl font-semibold text-foreground">
            {t("overviewTitle")}
          </h1>
          <p className="mt-0.5 text-xs text-muted">{metaLine}</p>
        </div>
      </header>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <RangeControl value={range} onChange={setRange} />
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-xs font-medium text-muted max-[1080px]:hidden">
            {t("overviewAgentSource")}
          </span>
          <AgentFilterControl
            value={agentFilter}
            onChange={setAgentFilter}
            ariaLabel={t("overviewAgentSource")}
          />
        </div>
      </div>

      {statsQ.isLoading ? (
        <StatsSkeleton />
      ) : statsQ.isError ? (
        <CenterMessage
          icon={<AlertTriangle size={28} />}
          title={t("cannotLoadData")}
          hint={t("cannotLoadDataHint", { error: errMessage(statsQ.error) })}
          action={
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void statsQ.refetch()}
              >
                {t("retry")}
              </Button>
              <Button size="sm" onClick={openSettings}>
                <Settings size={14} />
                {t("openSettings")}
              </Button>
            </div>
          }
        />
      ) : statsQ.data ? (
        <div
          className={cn(
            "space-y-5 transition-opacity duration-200",
            statsQ.isPlaceholderData && "opacity-60"
          )}
        >
          <StatsOverview stats={statsQ.data} agentFilter={agentFilter} />

          <div className="grid min-w-0 grid-cols-[minmax(0,1.6fr)_minmax(300px,0.9fr)] gap-3.5 max-[1200px]:grid-cols-1">
            <Card className="min-h-[240px] min-w-0">
              <CardHeader className="flex items-start justify-between">
                <div>
                  <CardTitle>{t("heatmapTitle")}</CardTitle>
                  <p className="mt-1 text-xs text-muted">
                    {t("promptCountMetric")}
                  </p>
                </div>
                <CalendarDays size={17} className="text-muted" />
              </CardHeader>
              <CardContent>
                <ActivityHeatmap
                  data={statsQ.data.byDay}
                  rangeStart={resolvedRange.start}
                  rangeEnd={resolvedRange.end}
                />
              </CardContent>
            </Card>
            <Card className="min-w-0">
              <CardHeader className="flex items-start justify-between">
                <div>
                  <CardTitle>{t("topActiveFolders")}</CardTitle>
                  <p className="mt-1 text-xs text-muted">
                    {t("sortedByPromptCount")}
                  </p>
                </div>
                <ListTree size={17} className="text-muted" />
              </CardHeader>
              <CardContent>
                <TopProjectsList data={statsQ.data.topProjects} />
              </CardContent>
            </Card>
          </div>
        </div>
      ) : null}

      <section>
        <div className="mb-2.5 flex items-center justify-between gap-3">
          <h2 className="text-sm font-semibold text-foreground">
            {t("recentPrompts")}
          </h2>
          <span className="text-[11px] text-muted">
            {t("recentRecordsCount", {
              count: formatNumber(recentItems.length),
            })}
          </span>
        </div>
        {recentQ.isLoading ? (
          <ListSkeleton />
        ) : recentQ.isError ? (
          <p className="text-xs text-muted">
            {t("loadFailedWithError", { error: errMessage(recentQ.error) })}
          </p>
        ) : recentItems.length > 0 ? (
          <PromptList
            items={recentItems}
            showProject
            showAgentBadge={agentFilter === "all"}
          />
        ) : (
          <p className="text-xs text-muted">{t("noPromptRecords")}</p>
        )}
      </section>

      {statsQ.data ? (
        <div
          className={cn(
            "space-y-5 transition-opacity duration-200",
            statsQ.isPlaceholderData && "opacity-60"
          )}
        >
          <div className="grid min-w-0 grid-cols-2 gap-3 max-[1200px]:grid-cols-1">
            <Card>
              <CardHeader>
                <CardTitle>{t("hourlyDistribution")}</CardTitle>
              </CardHeader>
              <CardContent>
                <HourChart data={statsQ.data.byHour} />
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle>{t("weekdayDistribution")}</CardTitle>
              </CardHeader>
              <CardContent>
                <WeekdayChart data={statsQ.data.byWeekday} />
              </CardContent>
            </Card>
          </div>

          <TokenStats usage={statsQ.data.usage} agentFilter={agentFilter} />
        </div>
      ) : null}
    </div>
  );
}
