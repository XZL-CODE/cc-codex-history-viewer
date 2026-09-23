import { memo } from "react";
import { Brain, Check, Copy } from "lucide-react";
import type { ChatMessage, ContentBlock } from "@/lib/types";
import { useCopy } from "@/hooks/useCopy";
import { useT } from "@/i18n";
import { absoluteTime, cn } from "@/lib/utils";
import { Badge } from "@/components/ui";
import { AgentBadge } from "@/components/AgentBadge";
import { Collapsible } from "./Collapsible";
import { Markdown } from "./Markdown";
import { MarkText } from "./MarkText";
import { AttachmentBlock, ToolResultBlock, ToolUseBlock } from "./ToolBlock";

export type RenderMode = "rendered" | "raw";

function TextBlock({
  block,
  markdown,
  regex,
}: {
  block: ContentBlock;
  markdown: boolean;
  regex: RegExp | null;
}) {
  const t = useT();
  const text = block.text ?? "";
  return (
    <div>
      {markdown ? (
        <Markdown text={text} regex={regex} />
      ) : (
        <div className="whitespace-pre-wrap break-words text-sm leading-relaxed text-foreground">
          <MarkText text={text} regex={regex} />
        </div>
      )}
      {block.truncated && (
        <p className="mt-1 text-[11px] text-warning">{t("blockTruncatedNote")}</p>
      )}
    </div>
  );
}

function ThinkingBlock({
  block,
  markdown,
  regex,
}: {
  block: ContentBlock;
  markdown: boolean;
  regex: RegExp | null;
}) {
  const t = useT();
  const text = block.text ?? "";
  return (
    <Collapsible
      summary={
        <>
          <Brain size={12} className="shrink-0 text-muted" />
          <span className="text-foreground">{t("thinkingLabel")}</span>
          <span className="min-w-0 truncate font-normal text-muted">
            {text.trim().split("\n")[0]?.slice(0, 90)}
          </span>
        </>
      }
    >
      <div className="px-3 py-2 text-muted">
        {markdown ? (
          <Markdown text={text} regex={regex} />
        ) : (
          <pre className="whitespace-pre-wrap break-words font-sans text-[12.5px] leading-relaxed">
            <MarkText text={text} regex={regex} />
          </pre>
        )}
      </div>
      {block.truncated && (
        <p className="px-3 pb-2 text-[11px] text-warning">{t("blockTruncatedNote")}</p>
      )}
    </Collapsible>
  );
}

function BlockView({
  block,
  markdown,
  regex,
}: {
  block: ContentBlock;
  markdown: boolean;
  regex: RegExp | null;
}) {
  const t = useT();
  switch (block.kind) {
    case "text":
      return <TextBlock block={block} markdown={markdown} regex={regex} />;
    case "thinking":
      return <ThinkingBlock block={block} markdown={markdown} regex={regex} />;
    case "tool_use":
      return <ToolUseBlock block={block} regex={regex} />;
    case "tool_result":
      return <ToolResultBlock block={block} regex={regex} />;
    case "attachment":
      return <AttachmentBlock block={block} regex={regex} />;
    case "image":
      return (
        <div className="text-xs text-muted">🖼 {block.text ?? t("imageFallback")}</div>
      );
    default:
      return null;
  }
}

function messagePlainText(message: ChatMessage): string {
  return message.blocks
    .filter((block) => block.kind === "text" && block.text)
    .map((block) => block.text as string)
    .join("\n\n");
}

export const MessageBubble = memo(function MessageBubble({
  message,
  highlighted,
  renderMode,
  regex,
}: {
  message: ChatMessage;
  highlighted: boolean;
  renderMode: RenderMode;
  regex: RegExp | null;
}) {
  const t = useT();
  const { copied, copy } = useCopy();
  const isUser = message.role === "user";
  // system = 斜杠命令的调用标记（如 /btw）；侧问命令的回复 CC 不持久化
  const isSystem = message.role === "system";
  // 用户输入保持原文；助手回复与思考按 Markdown 渲染
  const markdown = renderMode === "rendered" && !isUser && !isSystem;
  const copyable = messagePlainText(message);

  return (
    <div
      className={cn(
        "group rounded-lg border border-border bg-surface p-4 transition-shadow duration-500",
        isSystem && "border-dashed bg-surface/60",
        highlighted && "ring-2 ring-accent shadow-lg"
      )}
    >
      <div className="mb-2.5 flex items-center gap-2">
        <AgentBadge agent={message.agent} />
        {(isUser || isSystem) && (
          <Badge tone={isUser ? "accent" : "warning"}>
            {isUser ? t("roleUser") : t("commandBadge")}
          </Badge>
        )}
        {message.isSidechain && <Badge tone="muted">{t("sidechainBadge")}</Badge>}
        <span className="text-[11px] text-muted">
          {absoluteTime(message.timestamp)}
        </span>
        {copyable && (
          <button
            type="button"
            onClick={() => copy(copyable)}
            title={t("copyMessage")}
            className={cn(
              "ml-auto flex h-6 w-6 items-center justify-center rounded-md opacity-0 transition-opacity hover:bg-surface-2 group-hover:opacity-100 focus-visible:opacity-100",
              copied ? "text-success opacity-100" : "text-muted hover:text-accent"
            )}
          >
            {copied ? <Check size={12} /> : <Copy size={12} />}
          </button>
        )}
      </div>
      <div className="space-y-2">
        {message.blocks.map((block, index) => (
          <BlockView key={index} block={block} markdown={markdown} regex={regex} />
        ))}
      </div>
      {isSystem && message.agent === "claude" && (
        <p className="mt-2 text-[11px] text-muted">{t("commandReplyNote")}</p>
      )}
    </div>
  );
});
