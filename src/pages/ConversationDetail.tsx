import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  ArrowLeft,
  Check,
  ChevronsDownUp,
  ChevronsUpDown,
  Download,
  Folder,
  FolderOpen,
  GitBranch,
  MessageSquare,
  Search,
  Terminal,
} from "lucide-react";
import { useConversation } from "@/hooks/queries";
import { useCopy } from "@/hooks/useCopy";
import { getCurrentLang, useT } from "@/i18n";
import {
  Badge,
  Button,
  CenterMessage,
  Skeleton,
  Spinner,
} from "@/components/ui";
import type { Agent, ChatMessage, ConversationExportResult } from "@/lib/types";
import {
  absoluteTime,
  cn,
  encodePath,
  formatDuration,
  formatNumber,
  formatTokens,
  formatUsageCost,
  isMac,
  pathBasename,
  prettyPath,
} from "@/lib/utils";
import { api, errMessage } from "@/lib/api";
import { buildTokenRegex } from "@/lib/textMatch";
import { AgentBadge } from "@/components/AgentBadge";
import {
  CollapseContext,
  type CollapseSignal,
} from "@/components/conversation/Collapsible";
import { FindBar } from "@/components/conversation/FindBar";
import {
  MessageBubble,
  type RenderMode,
} from "@/components/conversation/MessageBubble";
import { buildOutline, Outline } from "@/components/conversation/Outline";

const BATCH_SIZE = 60;
const RENDER_MODE_KEY = "cchv-md-render";
const EMPTY_MESSAGES: ChatMessage[] = [];

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

export function ConversationDetail() {
  const { agent: agentParam, sessionId } = useParams();
  const agent: Agent = agentParam === "codex" ? "codex" : "claude";
  const navigate = useNavigate();
  const t = useT();
  const { data, isLoading, isError, error } = useConversation(
    agent,
    sessionId ?? null
  );
  const { copied, copy } = useCopy();

  // 从搜索结果跳转：?m=<消息 uuid> 优先，其次 ?t=<时间戳>；?q= 预填查找关键词
  const [searchParams] = useSearchParams();
  const targetTs = Number(searchParams.get("t")) || null;
  const targetUuid = searchParams.get("m");
  const initialFind = searchParams.get("q") ?? "";

  const messages = data?.messages ?? EMPTY_MESSAGES;

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
  }, [data]);
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
  }, [regex, data, renderMode]);
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

  // ---- 导出 Markdown ----
  const [exportOpen, setExportOpen] = useState(false);
  const [includeTools, setIncludeTools] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] =
    useState<ConversationExportResult | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  useEffect(() => {
    setExportResult(null);
    setExportError(null);
  }, [data?.sessionId, includeTools]);

  const handleExport = async () => {
    if (!data || exporting) return;
    setExporting(true);
    setExportError(null);
    setExportResult(null);
    try {
      const res = await api.exportConversation({
        agent: data.agent,
        sessionId: data.sessionId,
        includeTools,
        write: true,
        lang: getCurrentLang(),
      });
      setExportResult(res);
    } catch (e) {
      setExportError(errMessage(e));
    } finally {
      setExporting(false);
    }
  };

  const revealExported = async () => {
    if (exportResult?.path) {
      try {
        await api.revealPath(exportResult.path);
      } catch {
        /* 文件可能被移动，忽略 */
      }
    }
  };

  // 在终端中恢复该会话的命令
  const resumeCommand = data
    ? [
        data.project ? `cd "${data.project}" && ` : "",
        data.agent === "codex"
          ? `codex resume ${data.sessionId}`
          : `claude --resume ${data.sessionId}`,
      ].join("")
    : "";

  const shown = Math.min(visible, messages.length);
  const modKey = isMac ? "⌘" : "Ctrl+";

  return (
    <div className="mx-auto max-w-6xl px-4 py-5 sm:px-6 sm:py-6">
      <Button
        variant="ghost"
        size="sm"
        onClick={() => navigate(-1)}
        className="mb-4 -ml-2"
      >
        <ArrowLeft size={14} />
        {t("back")}
      </Button>

      {isLoading ? (
        <div className="space-y-3">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-28 w-full" />
          ))}
        </div>
      ) : isError ? (
        <CenterMessage
          icon={<MessageSquare size={28} />}
          title={t("cannotLoadConversation")}
          hint={errMessage(error)}
        />
      ) : data ? (
        <>
          <div className="mb-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h1 className="text-lg font-semibold text-foreground">
                {t("conversationDetailTitle")}
              </h1>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copy(resumeCommand)}
                  title={resumeCommand}
                >
                  {copied ? (
                    <Check size={13} className="text-success" />
                  ) : (
                    <Terminal size={13} />
                  )}
                  {copied ? t("copied") : t("copyResumeCommand")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setExportOpen((v) => !v)}
                  title={t("exportMarkdownTitle")}
                >
                  <Download size={13} />
                  {t("exportMarkdown")}
                </Button>
              </div>
            </div>

            {exportOpen && (
              <div className="mt-3 flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface px-3 py-2.5">
                <label className="flex cursor-pointer select-none items-center gap-1.5 text-xs text-foreground">
                  <input
                    type="checkbox"
                    checked={includeTools}
                    onChange={(e) => setIncludeTools(e.target.checked)}
                    className="accent-[var(--accent)]"
                  />
                  {t("includeToolsLabel")}
                </label>
                <Button size="sm" onClick={handleExport} disabled={exporting}>
                  {exporting ? (
                    <Spinner className="border-accent-fg/40 border-t-accent-fg" />
                  ) : (
                    <Download size={13} />
                  )}
                  {exporting ? t("exporting") : t("confirmExport")}
                </Button>
              </div>
            )}
            {exportError && (
              <p className="mt-2 text-xs text-danger">
                {t("exportFailed", { error: exportError })}
              </p>
            )}
            {exportResult && (
              <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                <Check size={13} className="shrink-0 text-success" />
                <span className="text-foreground">
                  {t("exportedMessages", {
                    count: formatNumber(exportResult.messageCount),
                  })}{" "}
                  <span
                    className="font-medium"
                    title={exportResult.path ?? undefined}
                  >
                    {exportResult.path
                      ? pathBasename(exportResult.path)
                      : t("notWrittenToFile")}
                  </span>
                </span>
                {exportResult.path && (
                  <button
                    onClick={revealExported}
                    className="flex items-center gap-1 text-accent transition-colors hover:underline"
                  >
                    <FolderOpen size={12} />
                    {t("revealInFinder")}
                  </button>
                )}
              </div>
            )}

            <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[11px] text-muted">
              <AgentBadge agent={data.agent} />
              {data.project && (
                <Link
                  to={`/project/${encodePath(data.project)}`}
                  className="flex items-center gap-1 transition-colors hover:text-accent"
                  title={data.project}
                >
                  <Folder size={11} />
                  {prettyPath(data.project)}
                </Link>
              )}
              {data.gitBranch && (
                <span className="flex items-center gap-1">
                  <GitBranch size={11} />
                  {data.gitBranch}
                </span>
              )}
              {data.cliVersion && (
                <Badge tone="muted">CLI {data.cliVersion}</Badge>
              )}
              {data.source && (
                <span>{t("conversationSource", { source: data.source })}</span>
              )}
              {data.models.length > 0 && (
                <span
                  className="max-w-[min(100%,24rem)] truncate"
                  title={data.models.join(", ")}
                >
                  {t("conversationModels", {
                    models: data.models.join(", "),
                  })}
                </span>
              )}
              <span>
                {absoluteTime(data.startedAt)} ~ {absoluteTime(data.endedAt)}
              </span>
              <span>
                · {t("messagesCount", { count: formatNumber(messages.length) })}
              </span>
              {data.endedAt > data.startedAt && (
                <span>
                  ·{" "}
                  {t("conversationDuration", {
                    duration: formatDuration(data.endedAt - data.startedAt),
                  })}
                </span>
              )}
              {data.usage.totalTokensIncludingCache > 0 && (
                <span
                  title={t("tokenTotalSuffix", {
                    value: formatNumber(data.usage.totalTokensIncludingCache),
                  })}
                >
                  ·{" "}
                  {t("conversationUsage", {
                    tokens: formatTokens(data.usage.totalTokensIncludingCache),
                    cost: formatUsageCost(data.usage),
                    messages: formatNumber(data.usage.assistantMessages),
                  })}
                </span>
              )}
            </div>
          </div>

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
      ) : null}
    </div>
  );
}
