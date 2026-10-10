// 对话消息查看器：渲染方式、全部展开 / 折叠、会话内查找（⌘/Ctrl+F）、分批渲染、
// URL 跳转参数（?m= / ?t= / ?q=）与跟随滚动的大纲。本机会话详情与外部会话详情共用，
// 页头内容（标题、元信息、用量面板等）由调用方通过 header 传入。

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useSearchParams } from "react-router-dom";
import {
  ChevronsDownUp,
  ChevronsUpDown,
  MessageSquare,
  Search,
} from "lucide-react";
import { useT } from "@/i18n";
import { Button, CenterMessage } from "@/components/ui";
import type { ChatMessage } from "@/lib/types";
import { cn, formatNumber, isMac } from "@/lib/utils";
import { buildTokenRegex } from "@/lib/textMatch";
import { CollapseContext, type CollapseSignal } from "./Collapsible";
import { FindBar } from "./FindBar";
import { MessageBubble, type RenderMode } from "./MessageBubble";
import { buildOutline, Outline } from "./Outline";

const BATCH_SIZE = 60;
const RENDER_MODE_KEY = "cchv-md-render";

function readRenderMode(): RenderMode {
  try {
    return localStorage.getItem(RENDER_MODE_KEY) === "raw" ? "raw" : "rendered";
  } catch {
    return "rendered";
  }
}

function sameElements(a: HTMLElement[], b: HTMLElement[]): boolean {
  return a.length === b.length && a.every((element, index) => element === b[index]);
}

/** 页头渲染时可用的上下文：用量面板需要轮次数、跳转函数与调用指纹索引 */
export interface ConversationViewHeaderContext {
  /** 大纲中的用户轮次数 */
  turns: number;
  /** 滚动到第 index 条消息并短暂高亮 */
  jumpTo: (index: number) => void;
  /** 调用指纹 → 承载该次调用的消息下标 */
  callIndexByKey: Map<string, number>;
}

export function ConversationView({
  messages,
  header,
  toolbarExtra,
}: {
  messages: ChatMessage[];
  /** 渲染在工具条上方 */
  header?: (ctx: ConversationViewHeaderContext) => ReactNode;
  /** 追加到工具条末尾 */
  toolbarExtra?: ReactNode;
}) {
  const t = useT();

  // 从搜索结果跳转：?m=<消息 uuid> 优先，其次 ?t=<时间戳>；?q= 预填查找关键词
  const [searchParams] = useSearchParams();
  const targetTs = Number(searchParams.get("t")) || null;
  const targetUuid = searchParams.get("m");
  const initialFind = searchParams.get("q") ?? "";

  // ---- 渲染方式：助手回复按 Markdown 渲染或显示原文 ----
  const [renderMode, setRenderMode] = useState<RenderMode>(readRenderMode);
  const chooseRenderMode = (mode: RenderMode) => {
    setRenderMode(mode);
    try {
      localStorage.setItem(RENDER_MODE_KEY, mode);
    } catch {
      /* 忽略持久化失败 */
    }
  };

  // ---- 全部展开 / 折叠 ----
  const [collapse, setCollapse] = useState<CollapseSignal>({
    forced: null,
    version: 0,
  });
  const expandAll = () =>
    setCollapse((signal) => ({ forced: "open", version: signal.version + 1 }));
  const collapseAll = () =>
    setCollapse((signal) => ({ forced: "closed", version: signal.version + 1 }));

  // ---- 分批渲染 ----
  const [visible, setVisible] = useState(BATCH_SIZE);
  useEffect(() => {
    setVisible(BATCH_SIZE);
  }, [messages]);
  const ensureVisible = useCallback((index: number) => {
    setVisible((value) => Math.max(value, index + BATCH_SIZE));
  }, []);
  const observerRef = useRef<IntersectionObserver | null>(null);
  const sentinelRef = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect();
    observerRef.current = null;
    if (!node) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible((value) => value + BATCH_SIZE);
        }
      },
      { rootMargin: "600px 0px" }
    );
    observer.observe(node);
    observerRef.current = observer;
  }, []);

  // ---- 会话内查找 ----
  const [findOpen, setFindOpen] = useState(initialFind.length > 0);
  const [findQuery, setFindQuery] = useState(initialFind);
  const [findFocus, setFindFocus] = useState(0);
  const regex = useMemo(
    () => (findOpen ? buildTokenRegex(findQuery.split(/\s+/)) : null),
    [findOpen, findQuery]
  );
  // 查找时渲染全部消息，这样命中才能被定位
  useEffect(() => {
    if (regex && messages.length > 0) setVisible(messages.length);
  }, [regex, messages.length]);
  const listRef = useRef<HTMLDivElement>(null);
  const [hits, setHits] = useState<HTMLElement[]>([]);
  const [activeHit, setActiveHit] = useState(0);
  useEffect(() => {
    setActiveHit(0);
  }, [regex]);
  useEffect(() => {
    const container = listRef.current;
    if (!container || !regex) {
      setHits([]);
      return;
    }
    let frame = 0;
    const collect = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const next = Array.from(
          container.querySelectorAll<HTMLElement>("mark.find-hit")
        );
        setHits((current) => (sameElements(current, next) ? current : next));
      });
    };
    collect();
    const observer = new MutationObserver(collect);
    observer.observe(container, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [regex, messages, renderMode]);
  useEffect(() => {
    hits.forEach((element, index) =>
      element.classList.toggle("find-hit-active", index === activeHit)
    );
    hits[activeHit]?.scrollIntoView({ block: "center" });
  }, [hits, activeHit]);
  const nextHit = () => {
    if (hits.length > 0) setActiveHit((index) => (index + 1) % hits.length);
  };
  const prevHit = () => {
    if (hits.length > 0) {
      setActiveHit((index) => (index - 1 + hits.length) % hits.length);
    }
  };
  const openFind = useCallback(() => {
    setFindOpen(true);
    setFindFocus((token) => token + 1);
  }, []);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = isMac ? event.metaKey : event.ctrlKey;
      if (mod && event.key.toLowerCase() === "f") {
        event.preventDefault();
        openFind();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openFind]);

  // ---- 定位目标消息（URL 参数或大纲点击）----
  const [jump, setJump] = useState<{ index: number; nonce: number } | null>(
    null
  );
  const handledJump = useRef<number | null>(null);
  const [highlightIdx, setHighlightIdx] = useState<number | null>(null);
  useEffect(() => {
    if (messages.length === 0) return;
    let index: number | null = null;
    if (targetUuid) {
      const found = messages.findIndex((message) => message.uuid === targetUuid);
      if (found >= 0) index = found;
    }
    if (index === null && targetTs) {
      let bestDiff = Number.POSITIVE_INFINITY;
      messages.forEach((message, position) => {
        const diff = Math.abs(message.timestamp - targetTs);
        if (diff < bestDiff) {
          bestDiff = diff;
          index = position;
        }
      });
    }
    if (index !== null) setJump({ index, nonce: Date.now() });
  }, [messages, targetTs, targetUuid]);
  useEffect(() => {
    if (jump) ensureVisible(jump.index);
  }, [jump, ensureVisible]);
  useEffect(() => {
    if (!jump || handledJump.current === jump.nonce || visible <= jump.index) return;
    handledJump.current = jump.nonce;
    const frame = requestAnimationFrame(() => {
      document
        .getElementById(`msg-${jump.index}`)
        ?.scrollIntoView({ block: "center" });
    });
    setHighlightIdx(jump.index);
    const timer = window.setTimeout(() => setHighlightIdx(null), 2500);
    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(timer);
    };
  }, [jump, visible]);
  const jumpTo = useCallback(
    (index: number) => setJump({ index, nonce: Date.now() }),
    []
  );

  const outline = useMemo(() => buildOutline(messages), [messages]);
  // 调用指纹 → 消息下标：用量面板的图表点击后跳到承载该次调用的消息
  const callIndexByKey = useMemo(() => {
    const map = new Map<string, number>();
    messages.forEach((message, index) => {
      if (message.usage) map.set(message.usage.callKey, index);
    });
    return map;
  }, [messages]);

  // 大纲跟随滚动：视口内最靠上的用户轮次为当前轮次；没有用户轮次在视口内时沿用上一个
  const [activeTurn, setActiveTurn] = useState<number | null>(null);
  useEffect(() => {
    if (outline.length === 0) return;
    const elements = outline
      .filter((turn) => turn.index < visible)
      .map((turn) => document.getElementById(`msg-${turn.index}`))
      .filter((element): element is HTMLElement => element !== null);
    if (elements.length === 0) return;
    const inView = new Set<number>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const index = Number(entry.target.id.replace("msg-", ""));
          if (entry.isIntersecting) inView.add(index);
          else inView.delete(index);
        }
        if (inView.size > 0) setActiveTurn(Math.min(...inView));
      },
      { rootMargin: "-10% 0px -60% 0px" }
    );
    elements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, [outline, visible]);

  const shown = Math.min(visible, messages.length);
  const modKey = isMac ? "⌘" : "Ctrl+";

  return (
    <>
      {header?.({ turns: outline.length, jumpTo, callIndexByKey })}

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div
          className="flex items-center rounded-lg border border-border bg-surface p-0.5"
          role="group"
          aria-label={t("renderModeTitle")}
          title={t("renderModeTitle")}
        >
          {(["rendered", "raw"] as RenderMode[]).map((mode) => (
            <button
              key={mode}
              type="button"
              aria-pressed={renderMode === mode}
              onClick={() => chooseRenderMode(mode)}
              className={cn(
                "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                renderMode === mode
                  ? "bg-accent text-accent-fg"
                  : "text-muted hover:text-foreground"
              )}
            >
              {mode === "rendered" ? t("renderMarkdown") : t("renderRaw")}
            </button>
          ))}
        </div>
        <Button variant="outline" size="sm" onClick={expandAll}>
          <ChevronsUpDown size={13} />
          {t("expandAll")}
        </Button>
        <Button variant="outline" size="sm" onClick={collapseAll}>
          <ChevronsDownUp size={13} />
          {t("collapseAll")}
        </Button>
        <Button variant="outline" size="sm" onClick={openFind}>
          <Search size={13} />
          {t("findButton")}
          <kbd className="rounded border border-border bg-background px-1 font-sans text-[10px] text-muted">
            {modKey}F
          </kbd>
        </Button>
        {toolbarExtra}
      </div>

      <div className="grid gap-5 min-[1200px]:grid-cols-[minmax(0,1fr)_224px] min-[1200px]:items-start">
        <div className="min-w-0">
          {findOpen && (
            <FindBar
              query={findQuery}
              onQueryChange={setFindQuery}
              current={hits.length > 0 ? activeHit : 0}
              total={hits.length}
              onPrev={prevHit}
              onNext={nextHit}
              onClose={() => setFindOpen(false)}
              focusToken={findFocus}
            />
          )}
          {messages.length === 0 ? (
            <CenterMessage
              icon={<MessageSquare size={28} />}
              title={t("noMessagesInSession")}
            />
          ) : (
            <CollapseContext.Provider value={collapse}>
              <div ref={listRef} className="space-y-3">
                {messages.slice(0, visible).map((message, index) => (
                  <div
                    key={message.uuid || index}
                    id={`msg-${index}`}
                    className="cv-auto"
                  >
                    <MessageBubble
                      message={message}
                      highlighted={highlightIdx === index}
                      renderMode={renderMode}
                      regex={regex}
                    />
                  </div>
                ))}
                {shown < messages.length && (
                  <>
                    <div ref={sentinelRef} aria-hidden className="h-px" />
                    <div className="flex items-center justify-center gap-3 py-2 text-[11px] text-muted">
                      <span>
                        {t("loadedMessages", {
                          shown: formatNumber(shown),
                          total: formatNumber(messages.length),
                        })}
                      </span>
                      <button
                        type="button"
                        onClick={() => setVisible(messages.length)}
                        className="font-medium text-accent hover:underline"
                      >
                        {t("showAllMessages")}
                      </button>
                    </div>
                  </>
                )}
              </div>
            </CollapseContext.Provider>
          )}
        </div>
        <Outline
          turns={outline}
          activeIndex={highlightIdx ?? activeTurn}
          onJump={jumpTo}
        />
      </div>
    </>
  );
}
