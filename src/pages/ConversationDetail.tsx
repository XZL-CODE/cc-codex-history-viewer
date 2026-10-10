import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
  Check,
  Download,
  Folder,
  FolderOpen,
  GitBranch,
  MessageSquare,
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
  encodePath,
  formatDuration,
  formatNumber,
  pathBasename,
  prettyPath,
} from "@/lib/utils";
import { api, errMessage } from "@/lib/api";
import { AgentBadge } from "@/components/AgentBadge";
import { ConversationView } from "@/components/conversation/ConversationView";
import { UsagePanel } from "@/components/conversation/UsagePanel";

const EMPTY_MESSAGES: ChatMessage[] = [];

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

  const messages = data?.messages ?? EMPTY_MESSAGES;

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
        <ConversationView
          messages={messages}
          header={({ turns, jumpTo, callIndexByKey }) => (
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
              </div>

              <UsagePanel
                agent={data.agent}
                usage={data.usage}
                breakdown={data.usageBreakdown}
                turns={turns}
                callIndexByKey={callIndexByKey}
                onJump={jumpTo}
              />
            </div>
          )}
        />
      ) : null}
    </div>
  );
}
