// 活跃度日历：GitHub 贡献图风格的单色阶热力图，按周排列，补齐没有记录的日期。

import { useEffect, useMemo, useRef, useState } from "react";
import {
  addDays,
  differenceInCalendarDays,
  format,
  parseISO,
  startOfDay,
  subWeeks,
} from "date-fns";
import { zhCN } from "date-fns/locale";
import type { DayCount } from "@/lib/types";
import { getCurrentLang, useT, type DictKey } from "@/i18n";
import { cn, dayLabel, formatNumber } from "@/lib/utils";

const CELL = 11;
const GAP = 3;
const STEP = CELL + GAP;
const GUTTER = 34;
const HEADER = 16;
const MAX_WEEKS = 53;
const WEEKDAY_KEYS: DictKey[] = [
  "weekdayMon",
  "weekdayTue",
  "weekdayWed",
  "weekdayThu",
  "weekdayFri",
  "weekdaySat",
  "weekdaySun",
];

function mondayOf(date: Date): Date {
  const day = startOfDay(date);
  return addDays(day, -((day.getDay() + 6) % 7));
}

function level(count: number, max: number): number {
  if (count <= 0 || max <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((count / max) * 4)));
}

interface Cell {
  key: string;
  date: Date;
  week: number;
  weekday: number;
  count: number;
}

interface Hover {
  x: number;
  y: number;
  cell: Cell;
}

export function ActivityHeatmap({
  data,
  rangeStart,
  rangeEnd,
}: {
  data: DayCount[];
  rangeStart: string | null;
  rangeEnd: string | null;
}) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<Hover | null>(null);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    setWidth(element.clientWidth);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) setWidth(entry.contentRect.width);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const counts = useMemo(
    () => new Map(data.map((row) => [row.day, row.count])),
    [data]
  );

  const grid = useMemo(() => {
    const today = startOfDay(new Date());
    const end = rangeEnd ? startOfDay(parseISO(rangeEnd)) : today;
    const start = rangeStart
      ? startOfDay(parseISO(rangeStart))
      : addDays(mondayOf(subWeeks(end, MAX_WEEKS - 1)), 0);
    let gridStart = mondayOf(start);
    let weeks = Math.ceil((differenceInCalendarDays(end, gridStart) + 1) / 7);
    const fit = Math.max(4, Math.floor((width - GUTTER) / STEP));
    if (weeks > fit) {
      gridStart = addDays(gridStart, (weeks - fit) * 7);
      weeks = fit;
    }
    const cells: Cell[] = [];
    let max = 0;
    let total = 0;
    let activeDays = 0;
    for (let week = 0; week < weeks; week += 1) {
      for (let weekday = 0; weekday < 7; weekday += 1) {
        const date = addDays(gridStart, week * 7 + weekday);
        if (date < start || date > end) continue;
        const key = format(date, "yyyy-MM-dd");
        const count = counts.get(key) ?? 0;
        max = Math.max(max, count);
        total += count;
        if (count > 0) activeDays += 1;
        cells.push({ key, date, week, weekday, count });
      }
    }
    const months: { week: number; label: string }[] = [];
    const locale = getCurrentLang() === "zh" ? zhCN : undefined;
    let lastLabelWeek = -10;
    let lastMonth = -1;
    for (let week = 0; week < weeks; week += 1) {
      const monday = addDays(gridStart, week * 7);
      const month = monday.getMonth();
      if (month !== lastMonth && week - lastLabelWeek >= 3) {
        months.push({ week, label: format(monday, t("heatmapMonthFormat"), { locale }) });
        lastLabelWeek = week;
      }
      lastMonth = month;
    }
    return { cells, weeks, max, total, activeDays, months };
  }, [counts, rangeStart, rangeEnd, width, t]);

  const svgWidth = GUTTER + grid.weeks * STEP;
  const svgHeight = HEADER + 7 * STEP;

  return (
    <div ref={containerRef} className="relative min-w-0">
      <p className="mb-2 text-xs text-muted">
        {grid.total > 0
          ? t("heatmapSummary", {
              days: formatNumber(grid.activeDays),
              total: formatNumber(grid.total),
              max: formatNumber(grid.max),
            })
          : t("heatmapEmpty")}
      </p>
      {width > 0 && (
        <svg
          width={svgWidth}
          height={svgHeight}
          role="img"
          aria-label={t("heatmapTitle")}
          className="block max-w-full select-none"
          onPointerLeave={() => setHover(null)}
        >
          {grid.months.map((month) => (
            <text
              key={`m-${month.week}`}
              x={GUTTER + month.week * STEP}
              y={10}
              className="fill-[var(--muted)] text-[10px]"
            >
              {month.label}
            </text>
          ))}
          {[0, 2, 4].map((weekday) => (
            <text
              key={`d-${weekday}`}
              x={0}
              y={HEADER + weekday * STEP + CELL - 2}
              className="fill-[var(--muted)] text-[10px]"
            >
              {t(WEEKDAY_KEYS[weekday])}
            </text>
          ))}
          {grid.cells.map((cell) => {
            const x = GUTTER + cell.week * STEP;
            const y = HEADER + cell.weekday * STEP;
            return (
              <rect
                key={cell.key}
                x={x}
                y={y}
                width={CELL}
                height={CELL}
                rx={2}
                className={cn("heat-cell", `heat-${level(cell.count, grid.max)}`)}
                onPointerEnter={() => setHover({ x, y, cell })}
              >
                <title>{`${dayLabel(cell.date.getTime())} · ${formatNumber(cell.count)} ${t("unitItems")}`}</title>
              </rect>
            );
          })}
        </svg>
      )}
      <div className="mt-2 flex items-center justify-end gap-1 text-[10px] text-muted">
        <span>{t("heatmapLess")}</span>
        {[0, 1, 2, 3, 4].map((step) => (
          <span
            key={step}
            className={cn("inline-block h-[10px] w-[10px] rounded-[2px]", `heat-${step}`)}
            aria-hidden
          />
        ))}
        <span>{t("heatmapMore")}</span>
      </div>
      {hover && (
        <div
          className="pointer-events-none absolute z-10 rounded-md border border-border bg-surface px-2 py-1 text-[11px] shadow-lg"
          style={{
            left: Math.min(hover.x + STEP, Math.max(0, width - 160)),
            top: hover.y + 28,
          }}
        >
          <strong className="text-foreground">
            {formatNumber(hover.cell.count)} {t("unitItems")}
          </strong>
          <span className="ml-1.5 text-muted">{dayLabel(hover.cell.date.getTime())}</span>
        </div>
      )}
    </div>
  );
}
