import { Calendar } from "lucide-react";
import { useT, type DictKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { RangePreset, StatsRange } from "@/lib/statsRange";

const PRESETS: { value: RangePreset; labelKey: DictKey }[] = [
  { value: "all", labelKey: "allTime" },
  { value: "7d", labelKey: "last7Days" },
  { value: "30d", labelKey: "last30Days" },
  { value: "month", labelKey: "thisMonth" },
  { value: "custom", labelKey: "rangeCustom" },
];

const inputClass =
  "h-8 rounded-lg border border-border bg-surface px-2 text-xs text-foreground outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-ring/20";

/** 统计范围：预设在前、自定义区间在后，作用于下方全部统计。 */
export function RangeControl({
  value,
  onChange,
}: {
  value: StatsRange;
  onChange: (next: StatsRange) => void;
}) {
  const t = useT();
  const invalid =
    value.preset === "custom" &&
    !!value.start &&
    !!value.end &&
    value.start > value.end;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="flex items-center gap-1 text-xs font-medium text-muted">
        <Calendar size={13} />
        {t("rangeLabel")}
      </span>
      <div
        className="inline-flex items-center rounded-lg border border-border bg-background p-0.5"
        role="group"
        aria-label={t("rangeLabel")}
      >
        {PRESETS.map((preset) => (
          <button
            key={preset.value}
            type="button"
            aria-pressed={value.preset === preset.value}
            onClick={() => onChange({ ...value, preset: preset.value })}
            className={cn(
              "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
              value.preset === preset.value
                ? "bg-accent text-accent-fg"
                : "text-muted hover:text-foreground"
            )}
          >
            {t(preset.labelKey)}
          </button>
        ))}
      </div>
      {value.preset === "custom" && (
        <div className="flex items-center gap-1.5">
          <input
            type="date"
            value={value.start ?? ""}
            max={value.end ?? undefined}
            onChange={(event) =>
              onChange({ ...value, start: event.target.value || null })
            }
            aria-label={t("startDate")}
            className={inputClass}
          />
          <span className="text-xs text-muted">~</span>
          <input
            type="date"
            value={value.end ?? ""}
            min={value.start ?? undefined}
            onChange={(event) =>
              onChange({ ...value, end: event.target.value || null })
            }
            aria-label={t("endDate")}
            className={inputClass}
          />
          {invalid && (
            <span className="text-xs text-danger">{t("invalidDateRange")}</span>
          )}
        </div>
      )}
    </div>
  );
}
