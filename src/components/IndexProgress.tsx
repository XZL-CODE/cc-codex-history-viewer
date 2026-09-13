// 索引构建进度：主区域顶部的细进度条，以及各处共用的进度文案。

import type { IndexProgress } from "@/lib/types";
import { useStore } from "@/store";
import { useT, type DictKey } from "@/i18n";
import { formatNumber } from "@/lib/utils";

type Translate = (
  key: DictKey,
  params?: Record<string, string | number>
) => string;

export function indexProgressLabel(
  progress: IndexProgress,
  t: Translate
): string {
  switch (progress.phase) {
    case "scanning":
      return t("indexingScanning");
    case "parsing":
      return t("indexingParsing", {
        done: formatNumber(progress.done),
        total: formatNumber(progress.total),
      });
    case "assembling":
      return t("indexingAssembling");
    default:
      return "";
  }
}

export function IndexProgressBar() {
  const { indexProgress } = useStore();
  const t = useT();
  if (!indexProgress) return null;
  const determinate =
    indexProgress.phase === "parsing" && indexProgress.total > 0;
  const percent = determinate
    ? Math.min(100, (indexProgress.done / indexProgress.total) * 100)
    : 0;
  return (
    <div
      role="progressbar"
      aria-label={indexProgressLabel(indexProgress, t)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={determinate ? Math.round(percent) : undefined}
      className="absolute inset-x-0 top-0 z-20 h-[3px] overflow-hidden bg-accent/15"
    >
      <div
        className={
          determinate
            ? "h-full bg-accent transition-[width] duration-150"
            : "progress-indeterminate h-full w-1/3 bg-accent"
        }
        style={determinate ? { width: `${percent}%` } : undefined}
      />
    </div>
  );
}
