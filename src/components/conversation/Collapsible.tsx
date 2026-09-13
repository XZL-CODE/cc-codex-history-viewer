import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

/** 「全部展开 / 全部折叠」广播：version 变化时每个折叠块按 forced 重置自身状态 */
export interface CollapseSignal {
  forced: "open" | "closed" | null;
  version: number;
}

export const CollapseContext = createContext<CollapseSignal>({
  forced: null,
  version: 0,
});

/**
 * 折叠块。正文只在展开时渲染，因此上千个工具调用的会话不会一次性把所有 JSON 塞进 DOM。
 */
export function Collapsible({
  summary,
  defaultOpen = false,
  children,
  className,
}: {
  summary: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
  className?: string;
}) {
  const signal = useContext(CollapseContext);
  const [open, setOpen] = useState(defaultOpen);

  useEffect(() => {
    if (signal.forced) setOpen(signal.forced === "open");
  }, [signal.version, signal.forced]);

  return (
    <div
      className={cn("rounded-lg border border-border bg-background", className)}
      data-collapsible={open ? "open" : "closed"}
    >
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex w-full items-center gap-1.5 px-3 py-1.5 text-left text-xs font-medium text-muted transition-colors hover:text-foreground"
      >
        <ChevronRight
          size={12}
          className={cn("shrink-0 transition-transform", open && "rotate-90")}
        />
        <span className="flex min-w-0 flex-1 items-center gap-1.5">{summary}</span>
      </button>
      {open && <div className="border-t border-border">{children}</div>}
    </div>
  );
}
