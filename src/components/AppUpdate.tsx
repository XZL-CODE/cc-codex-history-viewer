// 应用内更新的界面：顶栏的新版本提示、更新弹窗、设置里的「更新」一节。

import { lazy, Suspense, useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowUpCircle, ExternalLink, X } from "lucide-react";
import { useT } from "@/i18n";
import { useStore } from "@/store";
import { RELEASES_URL, useUpdater } from "@/updater";
import { absoluteTime, formatBytes } from "@/lib/utils";
import { Button, Spinner } from "./ui";

// 更新说明用对话里的 Markdown 组件渲染；它和语法高亮单独拆包，打开弹窗时才加载
const Markdown = lazy(() =>
  import("./conversation/Markdown").then((module) => ({
    default: module.Markdown,
  }))
);

/** Release 正文末尾是给手动下载的人看的安装说明，应用内更新用不上 */
const INSTALL_NOTES_HEADING = /^##\s+(安装说明|Install notes)/m;
/** 正文开头的「## v0.14.0 更新内容 / What's new」与弹窗自己的标题重复 */
const VERSION_HEADING = /^#{1,3}\s+v?\d+\.\d+\.\d+\b.*\n*/;

function releaseNotes(notes: string | null): string {
  if (!notes) return "";
  const cut = notes.search(INSTALL_NOTES_HEADING);
  const body = cut >= 0 ? notes.slice(0, cut) : notes;
  return body.trim().replace(VERSION_HEADING, "").trim();
}

function openReleasePage() {
  void openUrl(RELEASES_URL).catch(() => {});
}

/** 顶栏：发现新版本时出现，点开更新弹窗 */
export function UpdateButton() {
  const t = useT();
  const { available, openDialog } = useUpdater();
  if (!available) return null;
  return (
    <button
      type="button"
      onClick={openDialog}
      title={t("updateButtonTitle", { version: available.version })}
      className="flex h-9 shrink-0 items-center gap-1.5 rounded-lg border border-accent/40 bg-accent/15 px-2.5 text-xs font-medium text-accent transition-colors hover:bg-accent/25"
    >
      <ArrowUpCircle size={14} />
      <span className="max-[1220px]:hidden">
        {t("updateButton", { version: available.version })}
      </span>
    </button>
  );
}

function InstallProgress() {
  const t = useT();
  const { progress } = useUpdater();
  if (!progress || progress.phase === "installing") {
    return (
      <p className="flex items-center gap-2 text-xs text-muted">
        <Spinner />
        {progress ? t("updateInstalling") : t("updateStarting")}
      </p>
    );
  }
  const ratio = progress.total
    ? Math.min(1, progress.downloaded / progress.total)
    : null;
  return (
    <div className="space-y-1.5">
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-200"
          style={{ width: `${Math.round((ratio ?? 0) * 100)}%` }}
        />
      </div>
      <p className="text-[11px] text-muted">
        {progress.total
          ? t("updateDownloading", {
              done: formatBytes(progress.downloaded),
              total: formatBytes(progress.total),
            })
          : t("updateDownloadingUnknown", {
              done: formatBytes(progress.downloaded),
            })}
      </p>
    </div>
  );
}

/** 更新弹窗：版本、更新说明、立即更新；安装过程中不能关闭 */
export function UpdateDialog() {
  const t = useT();
  const {
    available,
    dialogOpen,
    closeDialog,
    install,
    installing,
    installError,
  } = useUpdater();

  useEffect(() => {
    if (!dialogOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDialog();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [dialogOpen, closeDialog]);

  if (!dialogOpen || !available) return null;

  const notes = releaseNotes(available.notes);
  const published = available.date ? Date.parse(available.date) : NaN;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={closeDialog}
        aria-hidden
      />

      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-dialog-title"
        className="relative flex max-h-[85vh] w-full max-w-lg flex-col rounded-lg border border-border bg-surface shadow-2xl"
      >
        <div className="flex items-start justify-between gap-3 border-b border-border px-5 py-3.5">
          <div className="min-w-0">
            <h2
              id="update-dialog-title"
              className="text-sm font-semibold text-foreground"
            >
              {t("updateDialogTitle", { version: available.version })}
            </h2>
            <p className="mt-0.5 text-[11px] text-muted">
              {t("updateCurrentVersion", { version: available.currentVersion })}
              {Number.isFinite(published) &&
                ` · ${t("updatePublishedAt", { date: absoluteTime(published) })}`}
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={closeDialog}
            disabled={installing}
            title={t("close")}
          >
            <X size={16} />
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <h3 className="mb-2 text-xs font-semibold text-foreground">
            {t("updateNotesTitle")}
          </h3>
          {notes ? (
            <Suspense
              fallback={
                <p className="whitespace-pre-wrap text-xs text-muted">
                  {notes}
                </p>
              }
            >
              <Markdown text={notes} />
            </Suspense>
          ) : (
            <p className="text-xs text-muted">{t("updateNoNotes")}</p>
          )}
        </div>

        <div className="space-y-2.5 border-t border-border px-5 py-3">
          {installing && <InstallProgress />}
          {installError && (
            <p className="break-words text-xs text-danger">
              {t("updateInstallFailed", { error: installError })}
            </p>
          )}
          <div className="flex flex-wrap items-center justify-between gap-2">
            <button
              type="button"
              onClick={openReleasePage}
              className="inline-flex items-center gap-1 text-xs text-muted transition-colors hover:text-foreground"
            >
              <ExternalLink size={12} />
              {t("updateViewRelease")}
            </button>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={closeDialog}
                disabled={installing}
              >
                {t("updateLater")}
              </Button>
              <Button
                size="sm"
                onClick={() => void install()}
                disabled={installing}
              >
                {installing && (
                  <Spinner className="border-accent-fg/40 border-t-accent-fg" />
                )}
                {t("updateInstallNow")}
              </Button>
            </div>
          </div>
          <p className="text-[11px] leading-relaxed text-muted">
            {t("updateInstallHint")}
          </p>
        </div>
      </div>
    </div>
  );
}

function CheckResult() {
  const t = useT();
  const { closeSettings } = useStore();
  const { status, checkError, available, openDialog } = useUpdater();
  if (status === "upToDate") {
    return <span className="text-xs text-muted">{t("updateUpToDate")}</span>;
  }
  if (status === "available" && available) {
    return (
      <span className="flex items-center gap-2 text-xs text-accent">
        {t("updateFound", { version: available.version })}
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            // 先关掉设置，避免两层弹窗叠在一起、Esc 一次关掉两个
            closeSettings();
            openDialog();
          }}
        >
          {t("updateView")}
        </Button>
      </span>
    );
  }
  if (status === "error") {
    return (
      <span className="flex min-w-0 flex-wrap items-center gap-2 text-xs">
        <span className="min-w-0 break-words text-danger">
          {t("updateCheckFailed", { error: checkError ?? "" })}
        </span>
        <Button variant="outline" size="sm" onClick={openReleasePage}>
          <ExternalLink size={12} />
          {t("updateOpenDownloadPage")}
        </Button>
      </span>
    );
  }
  return null;
}

/** 设置里的「更新」一节：当前版本、自动检查开关、手动检查 */
export function UpdateSettingsSection() {
  const t = useT();
  const { autoCheck, setAutoCheck, status, check } = useUpdater();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  return (
    <section className="space-y-3 rounded-lg border border-border p-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-xs font-semibold text-foreground">
          {t("updateSection")}
        </h3>
        {version && (
          <span className="text-[11px] text-muted">
            {t("updateCurrentVersion", { version })}
          </span>
        )}
      </div>
      <label className="flex cursor-pointer select-none items-start gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={autoCheck}
          onChange={(event) => setAutoCheck(event.target.checked)}
          className="mt-0.5 accent-[var(--accent)]"
        />
        <span>{t("updateAutoCheck")}</span>
      </label>
      <p className="text-[11px] leading-relaxed text-muted">
        {t("updatePrivacyHint")}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={status === "checking"}
          onClick={() => void check()}
        >
          {status === "checking" && <Spinner />}
          {status === "checking" ? t("updateChecking") : t("updateCheckNow")}
        </Button>
        <CheckResult />
      </div>
    </section>
  );
}
