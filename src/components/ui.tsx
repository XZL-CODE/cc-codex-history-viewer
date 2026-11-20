// 轻量基础组件（shadcn 风格，无 Radix 依赖）。

import {
  forwardRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

/* ----------------------------- Button ----------------------------- */

type ButtonVariant = "primary" | "ghost" | "outline" | "subtle" | "danger";
type ButtonSize = "sm" | "md" | "icon" | "icon-sm";

const btnVariants: Record<ButtonVariant, string> = {
  primary: "bg-accent text-accent-fg hover:opacity-90",
  ghost: "text-foreground hover:bg-surface-2",
  outline: "border border-border text-foreground hover:bg-surface-2",
  subtle: "bg-surface-2 text-foreground hover:bg-border",
  danger: "bg-danger text-white hover:opacity-90",
};

const btnSizes: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-xs gap-1.5",
  md: "h-9 px-4 text-sm gap-2",
  icon: "h-9 w-9",
  "icon-sm": "h-8 w-8",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "primary", size = "md", ...props }, ref) => (
    <button
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center rounded-lg font-medium transition-colors disabled:opacity-50 disabled:pointer-events-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        btnVariants[variant],
        btnSizes[size],
        className
      )}
      {...props}
    />
  )
);
Button.displayName = "Button";

/* ----------------------------- Input ----------------------------- */

export const Input = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement>
>(({ className, ...props }, ref) => (
  <input
    ref={ref}
    className={cn(
      "h-9 w-full rounded-lg border border-border bg-surface px-3 text-sm text-foreground placeholder:text-muted outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-ring/25",
      className
    )}
    {...props}
  />
));
Input.displayName = "Input";

/* ----------------------------- Card ----------------------------- */

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("rounded-lg border border-border bg-surface", className)}
      {...props}
    />
  );
}
export function CardHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("px-4 pt-4 pb-2", className)} {...props} />;
}
export function CardTitle({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("text-sm font-semibold text-foreground", className)}
      {...props}
    />
  );
}
export function CardContent({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("px-4 pb-4", className)} {...props} />;
}

/* ----------------------------- Badge ----------------------------- */

type BadgeTone =
  | "default"
  | "accent"
  | "muted"
  | "success"
  | "warning"
  | "info"
  | "outline";

const badgeTones: Record<BadgeTone, string> = {
  default: "bg-surface-2 text-foreground",
  accent: "bg-accent/15 text-accent",
  muted: "bg-surface-2 text-muted",
  success: "bg-success/15 text-success",
  warning: "bg-warning/15 text-warning",
  info: "bg-info/15 text-info",
  outline: "border border-border text-muted",
};

export function Badge({
  tone = "default",
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement> & { tone?: BadgeTone }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-none",
        badgeTones[tone],
        className
      )}
      {...props}
    />
  );
}

/* ----------------------------- Skeleton / Spinner ----------------------------- */

export function Skeleton({ className }: { className?: string }) {
  return (
    <div className={cn("animate-pulse rounded-md bg-surface-2", className)} />
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "h-4 w-4 animate-spin rounded-full border-2 border-border border-t-accent",
        className
      )}
    />
  );
}

/* ----------------------------- 状态占位 ----------------------------- */

export function CenterMessage({
  icon,
  title,
  hint,
  action,
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-20 text-center">
      {icon && <div className="text-muted">{icon}</div>}
      <div className="text-sm font-medium text-foreground">{title}</div>
      {hint && (
        <div className="max-w-md text-xs leading-relaxed text-muted">
          {hint}
        </div>
      )}
      {action}
    </div>
  );
}
