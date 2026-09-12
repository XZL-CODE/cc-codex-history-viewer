import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { DayCount, HourCount, WeekdayCount } from "@/lib/types";
import { useT, type DictKey } from "@/i18n";

const AXIS = "var(--muted)";
const GRID = "var(--border)";
const ACCENT = "var(--accent)";

interface TooltipProps {
  active?: boolean;
  payload?: { value?: number | string }[];
  label?: string | number;
}

function TooltipBox({ active, payload, label, unit }: TooltipProps & { unit?: string }) {
  const t = useT();
  if (!active || !payload || payload.length === 0) return null;
  return (
    <div className="rounded-lg border border-border bg-surface px-2.5 py-1.5 text-xs shadow-lg">
      <div className="font-medium text-foreground">{label}</div>
      <div className="text-muted">
        {payload[0]?.value} {unit ?? t("unitItems")}
      </div>
    </div>
  );
}

/** 最近 14 个有记录日期的活跃度。 */
export function ActivityChart({ data }: { data: DayCount[] }) {
  const recent = data.slice(-14);
  if (recent.length === 0) {
    return <EmptyChart />;
  }
  return (
    <ResponsiveContainer width="100%" height={174}>
      <BarChart data={recent} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
        <CartesianGrid stroke={GRID} strokeDasharray="3 3" vertical={false} />
        <XAxis
          dataKey="day"
          tick={{ fill: AXIS, fontSize: 10 }}
          tickFormatter={(d: string) => d.slice(5)}
          minTickGap={28}
          stroke={GRID}
        />
        <YAxis
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
          allowDecimals={false}
          width={32}
        />
        <Tooltip
          content={<TooltipBox />}
          cursor={{ fill: "var(--surface-2)" }}
        />
        <Bar dataKey="count" fill={ACCENT} radius={[3, 3, 0, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** 24 小时活跃分布 */
export function HourChart({ data }: { data: HourCount[] }) {
  const rows = data.map((h) => ({ name: `${h.hour}`, count: h.count }));
  return (
    <ResponsiveContainer width="100%" height={208}>
      <BarChart data={rows} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
        <CartesianGrid stroke={GRID} strokeDasharray="3 3" vertical={false} />
        <XAxis
          dataKey="name"
          tick={{ fill: AXIS, fontSize: 10 }}
          interval={1}
          stroke={GRID}
        />
        <YAxis
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
          allowDecimals={false}
          width={40}
        />
        <Tooltip
          content={<TooltipBox />}
          cursor={{ fill: "var(--surface-2)" }}
        />
        <Bar dataKey="count" fill={ACCENT} radius={[3, 3, 0, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

const WEEKDAY_KEYS: DictKey[] = [
  "weekdayMon",
  "weekdayTue",
  "weekdayWed",
  "weekdayThu",
  "weekdayFri",
  "weekdaySat",
  "weekdaySun",
];

/** 周一到周日的 Prompt 分布。 */
export function WeekdayChart({ data }: { data: WeekdayCount[] }) {
  const t = useT();
  const counts = new Map(data.map((row) => [row.weekday, row.count]));
  const rows = WEEKDAY_KEYS.map((key, weekday) => ({
    name: t(key),
    count: counts.get(weekday) ?? 0,
  }));
  return (
    <ResponsiveContainer width="100%" height={208}>
      <BarChart data={rows} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
        <CartesianGrid stroke={GRID} strokeDasharray="3 3" vertical={false} />
        <XAxis
          dataKey="name"
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
        />
        <YAxis
          tick={{ fill: AXIS, fontSize: 10 }}
          stroke={GRID}
          allowDecimals={false}
          width={40}
        />
        <Tooltip
          content={<TooltipBox />}
          cursor={{ fill: "var(--surface-2)" }}
        />
        <Bar dataKey="count" fill={ACCENT} radius={[3, 3, 0, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

function EmptyChart() {
  const t = useT();
  return (
    <div className="flex h-52 items-center justify-center text-xs text-muted">
      {t("noData")}
    </div>
  );
}
