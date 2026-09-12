// 概览统计的时间范围：预设按「今天」动态解析成 YYYY-MM-DD，自定义区间直接保存。

import { format, startOfMonth, subDays } from "date-fns";

export type RangePreset = "all" | "7d" | "30d" | "month" | "custom";

export interface StatsRange {
  preset: RangePreset;
  /** 仅 custom 使用 */
  start: string | null;
  end: string | null;
}

export interface ResolvedRange {
  start: string | null;
  end: string | null;
}

const STORAGE_KEY = "cchv-stats-range";
const PRESETS: RangePreset[] = ["all", "7d", "30d", "month", "custom"];

export const DEFAULT_RANGE: StatsRange = { preset: "all", start: null, end: null };

const fmt = (date: Date) => format(date, "yyyy-MM-dd");

export function resolveRange(range: StatsRange, today = new Date()): ResolvedRange {
  switch (range.preset) {
    case "7d":
      return { start: fmt(subDays(today, 6)), end: fmt(today) };
    case "30d":
      return { start: fmt(subDays(today, 29)), end: fmt(today) };
    case "month":
      return { start: fmt(startOfMonth(today)), end: fmt(today) };
    case "custom":
      return { start: range.start, end: range.end };
    default:
      return { start: null, end: null };
  }
}

export function readStatsRange(): StatsRange {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_RANGE;
    const parsed = JSON.parse(raw) as Partial<StatsRange>;
    const preset = PRESETS.includes(parsed.preset as RangePreset)
      ? (parsed.preset as RangePreset)
      : "all";
    const valid = (value: unknown) =>
      typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : null;
    return { preset, start: valid(parsed.start), end: valid(parsed.end) };
  } catch {
    return DEFAULT_RANGE;
  }
}

export function persistStatsRange(range: StatsRange) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(range));
  } catch {
    /* 忽略持久化失败 */
  }
}
