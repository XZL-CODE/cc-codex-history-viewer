// 外部会话列表：临时导入别人用本应用导出的单会话 Markdown，用同一套对话查看器阅读。
// 导入默认只在内存里，「保留」才把原始 Markdown 复制到应用数据目录；从不写入 ~/.claude / ~/.codex，
// 也不进入项目列表、统计与搜索。

import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  BookmarkCheck,
  Check,
  Clock,
  FileInput,
  Folder,
  Save,
  Trash2,
  X,
} from "lucide-react";
import {
  useExternalSessions,
  useRefreshExternalSessions,
} from "@/hooks/queries";
import { api, errMessage } from "@/lib/api";
import { useT } from "@/i18n";
import { plainSummary } from "@/lib/plainText";
import type {
  ExternalImportItem,
  ExternalImportResult,
  ExternalSessionSummary,
} from "@/lib/types";
import { absoluteTime, formatNumber, prettyPath } from "@/lib/utils";
import {
  Badge,
  Button,
  Card,
  CenterMessage,
  Skeleton,
  Spinner,
} from "@/components/ui";
import { AgentBadge } from "@/components/AgentBadge";

/** 临时 / 已保留 状态标签 */
export function ExternalStatusBadge({ kept }: { kept: boolean }) {
  const t = useT();
  return kept ? (
    <Badge tone="accent" title={t("externalStatusKeptTitle")}>
      <BookmarkCheck size={11} aria-hidden />
      {t("externalStatusKept")}
    </Badge>
  ) : (
    <Badge tone="muted" title={t("externalStatusTemporaryTitle")}>
      <Clock size={11} aria-hidden />
      {t("externalStatusTemporary")}
    </Badge>
  );
}

/** 删除按钮：requireConfirm 时第一次点击变为「确认删除」，再点才删除；几秒内不确认或失焦则复原 */
export function ExternalDeleteButton({
  kept,
  requireConfirm,
  disabled,
  onDelete,
}: {
  kept: boolean;
  requireConfirm: boolean;
  disabled?: boolean;
  onDelete: () => void;
}) {
  const t = useT();
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    if (!confirming) return;
    const timer = window.setTimeout(() => setConfirming(false), 4000);
    return () => window.clearTimeout(timer);
  }, [confirming]);

  const handleClick = (event: MouseEvent<HTMLButtonElement>) => {
    if (requireConfirm && !confirming) {
      setConfirming(true);
      return;
    }
    // 双击的第二下不算确认，否则两步确认形同虚设
    if (requireConfirm && event.detail > 1) return;
    setConfirming(false);
    onDelete();
  };

  return (
    <Button
      variant={confirming ? "danger" : "outline"}
      size="sm"
      onClick={handleClick}
      onBlur={() => setConfirming(false)}
      disabled={disabled}
      title={kept ? t("externalDeleteKeptTitle") : t("externalDeleteTitle")}
    >
      <Trash2 size={13} />
      {confirming ? t("externalConfirmDelete") : t("externalDelete")}
    </Button>
  );
}

const EMPTY_SESSIONS: ExternalSessionSummary[] = [];
const EMPTY_ERRORS: string[] = [];

function ListSkeleton() {
  return (
    <div className="space-y-2.5">
      {Array.from({ length: 4 }).map((_, index) => (
        <Skeleton key={index} className="h-20 w-full" />
      ))}
    </div>
  );
}

function ImportResultRow({
  item,
  title,
}: {
  item: ExternalImportItem;
  /** 所属条目的会话标题；找不到条目时为 null */
  title: string | null;
}) {
  const t = useT();
  return (
    <li className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
      {item.error ? (
        <AlertTriangle size={13} className="shrink-0 text-danger" />
      ) : (
        <Check size={13} className="shrink-0 text-success" />
      )}
      <span className="font-medium text-foreground" title={item.path}>
        {item.fileName}
      </span>
      {item.error ? (
        <span className="min-w-0 text-danger">{item.error}</span>
      ) : (
        <>
          <span className="text-muted">→</span>
          {item.id && (
            <Link
              to={`/external/${encodeURIComponent(item.id)}`}
              className="min-w-0 max-w-[min(100%,28rem)] truncate text-accent hover:underline"
            >
              {plainSummary(title ?? "") || t("untitledSession")}
            </Link>
          )}
          {item.variant && (
            <Badge tone="outline">
              {item.variant === "full"
                ? t("externalVariantFull")
                : t("externalVariantBrief")}
            </Badge>
          )}
          {item.replaced && (
            <Badge tone="warning">{t("externalReplaced")}</Badge>
          )}
          {item.kept && <ExternalStatusBadge kept />}
        </>
      )}
    </li>
  );
}

function ExternalSessionRow({
  session,
  busy,
  onKeep,
  onDelete,
}: {
  session: ExternalSessionSummary;
  busy: boolean;
  onKeep: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  return (
    <li className="rounded-lg border border-border bg-surface p-3.5 transition-colors hover:border-accent/40">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <Link
            to={`/external/${encodeURIComponent(session.id)}`}
            className="line-clamp-2 text-sm font-medium text-foreground transition-colors hover:text-accent"
          >
            {plainSummary(session.title) || t("untitledSession")}
          </Link>
          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
            <AgentBadge agent={session.agent} />
            {session.project && (
              <span
                className="flex min-w-0 items-center gap-1"
                title={session.project}
              >
                <Folder size={11} className="shrink-0" />
                <span className="truncate">{prettyPath(session.project)}</span>
              </span>
            )}
            <span>
              {absoluteTime(session.startedAt)} ~ {absoluteTime(session.endedAt)}
            </span>
            <span>
              {t("messagesCount", {
                count: formatNumber(session.messageCount),
              })}
            </span>
            <span className="flex flex-wrap items-center gap-1">
              {session.hasFull && (
                <Badge tone="outline" title={session.fullFileName ?? undefined}>
                  {t("externalVariantFull")}
                </Badge>
              )}
              {session.hasBrief && (
                <Badge tone="outline" title={session.briefFileName ?? undefined}>
                  {t("externalVariantBrief")}
                </Badge>
              )}
              <ExternalStatusBadge kept={session.kept} />
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {!session.kept && (
            <Button
              variant="outline"
              size="sm"
              onClick={onKeep}
              disabled={busy}
              title={t("externalKeepTitle")}
            >
              <Save size={13} />
              {t("externalKeep")}
            </Button>
          )}
          <ExternalDeleteButton
            kept={session.kept}
            requireConfirm={session.kept}
            disabled={busy}
            onDelete={onDelete}
          />
        </div>
      </div>
    </li>
  );
}

export function ExternalSessions() {
  const t = useT();
  const navigate = useNavigate();
  const { data, isLoading, isError, error } = useExternalSessions();
  const refresh = useRefreshExternalSessions();

  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] =
    useState<ExternalImportResult | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const sessions = data?.sessions ?? EMPTY_SESSIONS;
  const loadErrors = data?.loadErrors ?? EMPTY_ERRORS;
  const titleById = useMemo(
    () => new Map(sessions.map((session) => [session.id, session.title])),
    [sessions]
  );

  const pickAndImport = async () => {
    setImportError(null);
    let selected: string | string[] | null = null;
    try {
      selected = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "Markdown", extensions: ["md", "markdown"] }],
      });
    } catch (e) {
      setImportError(errMessage(e));
      return;
    }
    const paths =
      selected === null ? [] : Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) return;
    setImporting(true);
    setImportResult(null);
    try {
      const result = await api.importExternalMarkdown(paths);
      await refresh();
      // 成功的文件都属于同一条且没有失败的文件时直接打开它；否则留在列表页展示逐文件结果
      const okIds = new Set(
        result.items
          .filter((item) => !item.error && item.id)
          .map((item) => item.id as string)
      );
      const failed = result.items.some((item) => item.error);
      if (okIds.size === 1 && !failed) {
        const [id] = Array.from(okIds);
        navigate(`/external/${encodeURIComponent(id)}`);
        return;
      }
      setImportResult(result);
    } catch (e) {
      setImportError(errMessage(e));
    } finally {
      setImporting(false);
    }
  };

  const keep = async (id: string) => {
    setBusyId(id);
    setActionError(null);
    try {
      await api.keepExternalSession(id);
      await refresh();
    } catch (e) {
      setActionError(errMessage(e));
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (id: string) => {
    setBusyId(id);
    setActionError(null);
    try {
      await api.deleteExternalSession(id);
      await refresh(id);
    } catch (e) {
      setActionError(errMessage(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="page-content space-y-5 py-6">
      <header className="flex flex-wrap items-start justify-between gap-5">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <FileInput size={18} className="text-accent" />
            <h1 className="text-xl font-semibold text-foreground">
              {t("externalTitle")}
            </h1>
          </div>
          <p className="mt-1 max-w-3xl text-xs leading-relaxed text-muted">
            {t("externalIntro")}
          </p>
        </div>
        <Button
          onClick={() => void pickAndImport()}
          disabled={importing}
          className="shrink-0"
        >
          {importing ? (
            <Spinner className="border-accent-fg/40 border-t-accent-fg" />
          ) : (
            <FileInput size={14} />
          )}
          {importing ? t("externalImporting") : t("externalImportButton")}
        </Button>
      </header>

      {importError && (
        <p className="text-xs text-danger" role="alert">
          {t("externalImportFailed", { error: importError })}
        </p>
      )}

      {importResult && (
        <Card>
          <div className="flex items-center justify-between gap-2 px-4 pb-1 pt-3">
            <h2 className="text-xs font-semibold text-foreground">
              {t("externalImportResults")}
            </h2>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setImportResult(null)}
              title={t("dismiss")}
            >
              <X size={14} />
            </Button>
          </div>
          <ul className="space-y-1.5 px-4 pb-3.5">
            {importResult.items.map((item, index) => (
              <ImportResultRow
                key={`${item.path}-${index}`}
                item={item}
                title={item.id ? titleById.get(item.id) ?? null : null}
              />
            ))}
          </ul>
        </Card>
      )}

      {actionError && (
        <p className="text-xs text-danger" role="alert">
          {t("externalActionFailed", { error: actionError })}
        </p>
      )}

      {loadErrors.length > 0 && (
        <details className="rounded-lg border border-warning/40 bg-warning/5 px-3 py-2 text-[11px] text-warning">
          <summary className="cursor-pointer select-none">
            {t("externalLoadErrors", { count: loadErrors.length })}
          </summary>
          <ul className="mt-1.5 space-y-0.5 text-muted">
            {loadErrors.map((message, index) => (
              <li key={index} className="break-all">
                {message}
              </li>
            ))}
          </ul>
        </details>
      )}

      {isLoading ? (
        <ListSkeleton />
      ) : isError ? (
        <CenterMessage
          icon={<FileInput size={28} />}
          title={t("loadFailed")}
          hint={errMessage(error)}
        />
      ) : sessions.length === 0 ? (
        <CenterMessage
          icon={<FileInput size={28} />}
          title={t("externalEmpty")}
          hint={t("externalEmptyHint")}
        />
      ) : (
        <ul className="space-y-2.5">
          {sessions.map((session) => (
            <ExternalSessionRow
              key={session.id}
              session={session}
              busy={busyId === session.id}
              onKeep={() => void keep(session.id)}
              onDelete={() => void remove(session.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
