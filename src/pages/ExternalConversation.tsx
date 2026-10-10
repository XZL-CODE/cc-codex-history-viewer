// 外部会话详情：用本机会话同一套对话查看器阅读导入的 Markdown。
// 只读文件里有的信息：没有用量明细、导出与 Resume 命令；「完整 / 只看对话」切换详细版与简洁版。

import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
  Check,
  Copy,
  FileInput,
  Folder,
  GitBranch,
  MessageSquare,
  Save,
} from "lucide-react";
import {
  useExternalConversation,
  useRefreshExternalSessions,
} from "@/hooks/queries";
import { useCopy } from "@/hooks/useCopy";
import { useT } from "@/i18n";
import {
  Badge,
  Button,
  CenterMessage,
  Skeleton,
  Spinner,
} from "@/components/ui";
import type { ChatMessage, ExternalView } from "@/lib/types";
import {
  absoluteTime,
  cn,
  formatCost,
  formatDuration,
  formatNumber,
  prettyPath,
} from "@/lib/utils";
import { api, errMessage } from "@/lib/api";
import { AgentBadge } from "@/components/AgentBadge";
import { ConversationView } from "@/components/conversation/ConversationView";
import {
  ExternalDeleteButton,
  ExternalStatusBadge,
} from "@/pages/ExternalSessions";

const VIEW_KEY = "cchv-external-view";
const EMPTY_MESSAGES: ChatMessage[] = [];

function readView(): ExternalView {
  try {
    return localStorage.getItem(VIEW_KEY) === "dialogue" ? "dialogue" : "full";
  } catch {
    return "full";
  }
}

export function ExternalConversation() {
  const { id } = useParams();
  const navigate = useNavigate();
  const t = useT();
  const { copied, copy } = useCopy();
  const refresh = useRefreshExternalSessions();

  // ---- 显示范围：完整（详细版）/ 只看对话（简洁版，或由详细版过滤）----
  const [view, setView] = useState<ExternalView>(readView);
  const chooseView = (next: ExternalView) => {
    setView(next);
    try {
      localStorage.setItem(VIEW_KEY, next);
    } catch {
      /* 忽略持久化失败 */
    }
  };

  const { data, isLoading, isError, error, isPlaceholderData } =
    useExternalConversation(id ?? null, view);
  const messages = data?.detail.messages ?? EMPTY_MESSAGES;
  // 切换期间沿用上一份内容，按钮先跟随用户的选择；加载完成后以后端实际返回的范围为准
  const shownView: ExternalView =
    data && !isPlaceholderData ? data.view : view;

  // ---- 保留 / 删除 ----
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const keep = async () => {
    if (!data || busy) return;
    setBusy(true);
    setActionError(null);
    try {
      await api.keepExternalSession(data.summary.id);
      await refresh();
    } catch (e) {
      setActionError(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!data || busy) return;
    const entryId = data.summary.id;
    setBusy(true);
    setActionError(null);
    try {
      await api.deleteExternalSession(entryId);
    } catch (e) {
      setActionError(errMessage(e));
      setBusy(false);
      return;
    }
    // 先离开详情页再刷新，已删除条目的详情直接移出缓存
    navigate("/external", { replace: true });
    void refresh(entryId);
  };

  const summary = data?.summary;
  const detail = data?.detail;
  const sourceFiles = summary
    ? [
        summary.fullFileName
          ? `${t("externalVariantFull")} ${summary.fullFileName}`
          : null,
        summary.briefFileName
          ? `${t("externalVariantBrief")} ${summary.briefFileName}`
          : null,
      ].filter((part): part is string => part !== null)
    : [];

  // 工具条末尾的「完整 / 只看对话」；只导入简洁版时「完整」不可选
  const viewToggle = summary ? (
    <div className="flex items-center gap-2">
      <div
        className="flex items-center rounded-lg border border-border bg-surface p-0.5"
        role="group"
        aria-label={t("externalViewLabel")}
        title={t("externalViewLabel")}
      >
        {(["full", "dialogue"] as ExternalView[]).map((mode) => {
          const unavailable = mode === "full" && !summary.hasFull;
          const hint = unavailable
            ? t("externalFullUnavailable")
            : mode === "dialogue" && !summary.hasBrief
              ? t("externalDialogueDerived")
              : undefined;
          return (
            <button
              key={mode}
              type="button"
              aria-pressed={shownView === mode}
              aria-disabled={unavailable || undefined}
              title={hint}
              onClick={() => {
                // 已在显示的范围不重新请求，免得丢掉已加载的批次与滚动位置
                if (!unavailable && mode !== shownView) chooseView(mode);
              }}
              className={cn(
                "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                shownView === mode
                  ? "bg-accent text-accent-fg"
                  : "text-muted hover:text-foreground",
                unavailable && "cursor-not-allowed opacity-50 hover:text-muted"
              )}
            >
              {mode === "full" ? t("externalViewFull") : t("externalViewDialogue")}
            </button>
          );
        })}
      </div>
      {isPlaceholderData && <Spinner />}
    </div>
  ) : null;

  const header =
    summary && detail ? (
      <div className="mb-4">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="min-w-0 flex-1">
            <h1 className="line-clamp-2 text-lg font-semibold text-foreground">
              {summary.title || t("untitledSession")}
            </h1>
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              <AgentBadge agent={summary.agent} />
              <Badge tone="outline">
                <FileInput size={11} aria-hidden />
                {t("externalBadge")}
              </Badge>
              <ExternalStatusBadge kept={summary.kept} />
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {!summary.kept && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => void keep()}
                disabled={busy}
                title={t("externalKeepTitle")}
              >
                <Save size={13} />
                {t("externalKeep")}
              </Button>
            )}
            <ExternalDeleteButton
              kept={summary.kept}
              requireConfirm
              disabled={busy}
              onDelete={() => void remove()}
            />
          </div>
        </div>

        {actionError && (
          <p className="mt-2 text-xs text-danger" role="alert">
            {t("externalActionFailed", { error: actionError })}
          </p>
        )}

        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[11px] text-muted">
          {detail.project && (
            <span className="flex items-center gap-1" title={detail.project}>
              <Folder size={11} />
              {prettyPath(detail.project)}
            </span>
          )}
          {detail.gitBranch && (
            <span className="flex items-center gap-1">
              <GitBranch size={11} />
              {detail.gitBranch}
            </span>
          )}
          {detail.cliVersion && (
            <Badge tone="muted">CLI {detail.cliVersion}</Badge>
          )}
          <span>
            {absoluteTime(detail.startedAt)} ~ {absoluteTime(detail.endedAt)}
          </span>
          <span>
            · {t("messagesCount", { count: formatNumber(messages.length) })}
          </span>
          {detail.endedAt > detail.startedAt && (
            <span>
              ·{" "}
              {t("conversationDuration", {
                duration: formatDuration(detail.endedAt - detail.startedAt),
              })}
            </span>
          )}
          {summary.tokens != null && summary.tokens > 0 && (
            <span>
              ·{" "}
              {t("externalTokens", { value: formatNumber(summary.tokens) })}
            </span>
          )}
          {summary.costUsd != null && (
            <span>
              ·{" "}
              {t("externalCost", { value: formatCost(summary.costUsd) })}
            </span>
          )}
        </div>

        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[11px] text-muted">
          {summary.sessionId && (
            <span className="flex min-w-0 items-center gap-1">
              <span className="break-all font-mono">
                {t("externalSessionId", { id: summary.sessionId })}
              </span>
              <button
                type="button"
                onClick={() => void copy(summary.sessionId)}
                title={copied ? t("copied") : t("copySessionId")}
                className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted transition-colors hover:bg-surface-2 hover:text-foreground"
              >
                {copied ? (
                  <Check size={12} className="text-success" />
                ) : (
                  <Copy size={12} />
                )}
              </button>
            </span>
          )}
          {sourceFiles.length > 0 && (
            <span className="max-w-full truncate" title={sourceFiles.join(" · ")}>
              {t("externalSourceFiles", { files: sourceFiles.join(" · ") })}
            </span>
          )}
        </div>
      </div>
    ) : null;

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
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => navigate("/external")}
            >
              <FileInput size={13} />
              {t("externalBackToList")}
            </Button>
          }
        />
      ) : data ? (
        <ConversationView
          messages={messages}
          header={() => header}
          toolbarExtra={viewToggle}
        />
      ) : null}
    </div>
  );
}
