import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** 指标卡：一个数字加一到三行说明；首页 Token 统计与会话用量明细共用 */
export function StatCard({
  icon,
  label,
  value,
  sub,
  prominent = false,
  className,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  /** 说明文字；传多个 `<span className="block">` 可分行 */
  sub?: ReactNode;
  prominent?: boolean;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border p-4",
        prominent ? "border-accent/50 bg-accent/5" : "border-border bg-surface",
        className
      )}
    >
      <div className="flex items-center gap-1.5 text-xs text-muted">
        {icon}
        <span>{label}</span>
      </div>
      <div className="mt-1.5 text-2xl font-semibold text-foreground">{value}</div>
      {sub && <div className="mt-0.5 text-[11px] leading-relaxed text-muted">{sub}</div>}
    </div>
  );
}
