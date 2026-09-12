import { useEffect, useRef, type KeyboardEvent } from "react";
import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import { useT } from "@/i18n";
import { Button } from "@/components/ui";

export function FindBar({
  query,
  onQueryChange,
  current,
  total,
  onPrev,
  onNext,
  onClose,
  focusToken,
}: {
  query: string;
  onQueryChange: (value: string) => void;
  current: number;
  total: number;
  onPrev: () => void;
  onNext: () => void;
  onClose: () => void;
  /** 变化时把焦点交给输入框（⌘F 重复触发也能重新聚焦） */
  focusToken: number;
}) {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusToken]);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) onPrev();
      else onNext();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };

  return (
    <div className="sticky top-2 z-20 mb-3 flex items-center gap-1.5 rounded-lg border border-border bg-surface px-2 py-1.5 shadow-md">
      <Search size={14} className="shrink-0 text-muted" />
      <input
        ref={inputRef}
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t("findPlaceholder")}
        className="h-7 min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted"
      />
      <span className="shrink-0 tabular-nums text-[11px] text-muted">
        {query.trim()
          ? total > 0
            ? t("findCount", { current: current + 1, total })
            : t("findNoHits")
          : ""}
      </span>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onPrev}
        disabled={total === 0}
        title={t("findPrev")}
      >
        <ChevronUp size={14} />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onNext}
        disabled={total === 0}
        title={t("findNext")}
      >
        <ChevronDown size={14} />
      </Button>
      <Button variant="ghost" size="icon-sm" onClick={onClose} title={t("closeFind")}>
        <X size={14} />
      </Button>
    </div>
  );
}
