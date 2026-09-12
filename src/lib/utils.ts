import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { format, formatDistanceToNow } from "date-fns";
import { zhCN } from "date-fns/locale";
import { getCurrentLang, translate } from "@/i18n";

/** 合并 Tailwind 类名 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 相对时间，如「3 天前」/ "3 days ago"（跟随当前界面语言） */
export function relativeTime(ts: number): string {
  if (!ts) return translate("unknownTime");
  try {
    return formatDistanceToNow(new Date(ts), {
      addSuffix: true,
      locale: getCurrentLang() === "zh" ? zhCN : undefined,
    });
  } catch {
    return translate("unknownTime");
  }
}

/** 绝对时间，如「2026-05-16 18:30」 */
export function absoluteTime(ts: number): string {
  if (!ts) return "—";
  try {
    return format(new Date(ts), "yyyy-MM-dd HH:mm");
  } catch {
    return "—";
  }
}

/** 日期，如「2026年5月16日」/ "May 16, 2026"（跟随当前界面语言） */
export function dayLabel(ts: number): string {
  if (!ts) return "—";
  try {
    return format(new Date(ts), translate("dayLabelFormat"));
  } catch {
    return "—";
  }
}

/** 千分位数字 */
export function formatNumber(n: number): string {
  return (n ?? 0).toLocaleString(getCurrentLang() === "zh" ? "zh-CN" : "en-US");
}

/** Token 数缩写：≥1e9 → "1.2B"，≥1e6 → "3.4M"，≥1e3 → "5.6k"，否则原样 */
export function formatTokens(n: number): string {
  const v = n ?? 0;
  const fmt = (x: number, suffix: string) => {
    const s = x.toFixed(1);
    return `${s.endsWith(".0") ? s.slice(0, -2) : s}${suffix}`;
  };
  if (v >= 1e9) return fmt(v / 1e9, "B");
  if (v >= 1e6) return fmt(v / 1e6, "M");
  if (v >= 1e3) return fmt(v / 1e3, "k");
  return String(v);
}

/** 把绝对路径压缩为可读短路径：/Users/xxx/... 与 C:\Users\xxx\... → ~... */
export function prettyPath(path: string): string {
  if (!path) return "";
  return path
    .replace(/^\/Users\/[^/]+/, "~")
    .replace(/^\/home\/[^/]+/, "~")
    .replace(/^[A-Za-z]:[\\/]Users[\\/][^\\/]+/, "~");
}

/** 路径最后一段，同时接受 / 与 \ 分隔符 */
export function pathBasename(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : path;
}

/** 当前是否 macOS（决定快捷键修饰键） */
export const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** 快捷键提示里的修饰键标签 */
export const modKeyLabel = isMac ? "⌘" : "Ctrl";

/** react-router 路由参数编码 */
export function encodePath(path: string): string {
  return encodeURIComponent(path);
}
export function decodePath(param: string): string {
  try {
    return decodeURIComponent(param);
  } catch {
    return param;
  }
}

/** 两个时间戳之间的天数（含两端） */
export function daysSpan(from: number, to: number): number {
  if (!from || !to || to < from) return 0;
  return Math.floor((to - from) / 86_400_000) + 1;
}
