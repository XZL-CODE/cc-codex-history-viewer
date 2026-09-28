import { ListTree } from "lucide-react";
import type { ChatMessage } from "@/lib/types";
import { useT } from "@/i18n";
import { absoluteTime, cn, formatCost, formatTokens, usageTotal } from "@/lib/utils";

export interface OutlineTurn {
  index: number;
  message: ChatMessage;
  preview: string;
  /** 本轮（到下一个用户轮次之前）归属本会话的调用 Token 总量，含缓存 */
  tokens: number;
  /** 本轮估算成本；没有已知定价的调用时为 null */
  cost: number | null;
}

export function buildOutline(messages: ChatMessage[]): OutlineTurn[] {
  const turns: OutlineTurn[] = [];
  messages.forEach((message, index) => {
    if (message.role !== "user") return;
    const text = message.blocks
      .filter((block) => block.kind === "text" && block.text)
      .map((block) => block.text as string)
      .join(" ")
      .replace(/\s+/g, " ")
      .trim();
    if (!text) return;
    turns.push({
      index,
      message,
      preview: text.length > 90 ? `${text.slice(0, 90)}…` : text,
      tokens: 0,
      cost: null,
    });
  });
  // 一轮 = 从这条用户消息到下一条用户消息之前；复制自其他会话的调用不计
  turns.forEach((turn, position) => {
    const end = turns[position + 1]?.index ?? messages.length;
    let tokens = 0;
    let cost = 0;
    let priced = false;
    for (let i = turn.index; i < end; i += 1) {
      const usage = messages[i].usage;
      if (!usage || !usage.attributed) continue;
      tokens += usageTotal(usage);
      if (usage.estCostUsd !== null) {
        cost += usage.estCostUsd;
        priced = true;
      }
    }
    turn.tokens = tokens;
    turn.cost = priced ? cost : null;
  });
  return turns;
}

/** 用户轮次大纲：点击跳到对应消息 */
export function Outline({
  turns,
  activeIndex,
  onJump,
}: {
  turns: OutlineTurn[];
  activeIndex: number | null;
  onJump: (index: number) => void;
}) {
  const t = useT();
  return (
    <aside className="sticky top-4 hidden max-h-[calc(100vh-7rem)] flex-col overflow-hidden rounded-lg border border-border bg-surface min-[1200px]:flex">
      <div className="flex items-center gap-1.5 border-b border-border px-3 py-2 text-xs font-semibold text-foreground">
        <ListTree size={13} className="text-muted" />
        {t("outlineTitle")}
        <span className="ml-auto text-[11px] font-normal text-muted">
          {turns.length}
        </span>
      </div>
      {turns.length === 0 ? (
        <p className="px-3 py-6 text-center text-[11px] text-muted">
          {t("outlineEmpty")}
        </p>
      ) : (
        <ol className="min-h-0 flex-1 overflow-y-auto py-1">
          {turns.map((turn, position) => (
            <li key={turn.index}>
              <button
                type="button"
                onClick={() => onJump(turn.index)}
                title={turn.preview}
                className={cn(
                  "block w-full px-3 py-1.5 text-left transition-colors hover:bg-surface-2/70",
                  activeIndex === turn.index && "bg-accent/10"
                )}
              >
                <span className="flex items-center gap-1.5 text-[10px] text-muted">
                  <span className="font-semibold text-accent">#{position + 1}</span>
                  <span>{absoluteTime(turn.message.timestamp)}</span>
                </span>
                <span className="mt-0.5 line-clamp-2 text-[11.5px] leading-snug text-foreground">
                  {turn.preview}
                </span>
                {turn.tokens > 0 && (
                  <span className="mt-0.5 block text-[10px] tabular-nums text-muted">
                    {formatTokens(turn.tokens)} · {formatCost(turn.cost)}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ol>
      )}
    </aside>
  );
}
