// 导入 Claude Code 会话：选 zip → 项目映射 → 生成计划 →（有冲突时逐个选择）→ 写入并重建索引 → 汇总。
// 前两步只读；取消冲突弹窗时什么都不写。

import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Check,
  FileArchive,
  FolderOpen,
  FolderSearch,
  X,
} from "lucide-react";
import { useSettings } from "@/hooks/queries";
import { api, errMessage } from "@/lib/api";
import { useT, type DictKey } from "@/i18n";
import { useStore } from "@/store";
import type {
  ConflictDecision,
  ImportAction,
  ImportInspection,
  ImportPlan,
  ImportResult,
  PlannedSession,
  ProjectMapping,
  SessionSide,
} from "@/lib/types";
import {
  absoluteTime,
  cn,
  encodePath,
  formatNumber,
  prettyPath,
} from "@/lib/utils";
import { Badge, Button, Input, Spinner } from "@/components/ui";
import { indexProgressLabel } from "./IndexProgress";

type Phase =
  | "pick"
  | "inspecting"
  | "mapping"
  | "planning"
  | "conflicts"
  | "applying"
  | "done";

interface MappingRow {
  cloudDir: string;
  cloudCwd: string;
  sessionCount: number;
  /** 本机目录；空串表示保持原路径 */
  local: string;
  source: "remembered" | "name" | null;
}

const ACTION_KEY: Record<ImportAction, DictKey> = {
  add: "importActionAdd",
  update: "importActionUpdate",
  skip_identical: "importActionSkipIdentical",
  skip_older: "importActionSkipOlder",
  conflict: "importActionConflict",
};

const ACTION_TONE: Record<
  ImportAction,
  "success" | "accent" | "muted" | "warning"
> = {
  add: "success",
  update: "accent",
  skip_identical: "muted",
  skip_older: "muted",
  conflict: "warning",
};

function sessionKey(session: { cloudDir: string; sessionId: string }) {
  return `${session.cloudDir}/${session.sessionId}`;
}

function rowsFromInspection(inspection: ImportInspection): MappingRow[] {
  return inspection.projects.map((project) => ({
    cloudDir: project.cloudDir,
    cloudCwd: project.cloudCwd,
    sessionCount: project.sessionCount,
    local: project.suggestedLocal ?? "",
    source: project.suggestionSource,
  }));
}

function mappingsFromRows(rows: MappingRow[]): ProjectMapping[] {
  return rows.map((row) => ({
    cloudCwd: row.cloudCwd,
    localCwd: row.local.trim() ? row.local.trim() : null,
  }));
}

function SideSummary({
  label,
  side,
}: {
  label: string;
  side: SessionSide;
}) {
  const t = useT();
  return (
    <span className="text-[11px] text-muted">
      <span className="font-medium text-foreground">{label}</span>{" "}
      {t("importSideSummary", {
        lines: formatNumber(side.lines),
        time: side.lastTimestamp ? absoluteTime(side.lastTimestamp) : "—",
      })}
    </span>
  );
}

function SessionTitle({ session }: { session: PlannedSession }) {
  const t = useT();
  return (
    <div className="min-w-0">
      <div className="truncate text-xs font-medium text-foreground">
        {session.title || t("untitledSession")}
      </div>
      <div className="mt-0.5 flex flex-wrap items-center gap-x-2 text-[11px] text-muted">
        <span className="font-mono" title={session.sessionId}>
          {session.sessionId.slice(0, 8)}
        </span>
        <span className="truncate" title={session.targetCwd}>
          {prettyPath(session.targetCwd)}
        </span>
      </div>
      {session.existingElsewhere && (
        <p className="mt-0.5 text-[11px] text-warning">
          {t("importExistingElsewhere", { dir: session.existingElsewhere })}
        </p>
      )}
    </div>
  );
}

function SkippedEntries({ entries }: { entries: string[] }) {
  const t = useT();
  if (entries.length === 0) return null;
  return (
    <details className="rounded-lg border border-border bg-surface-2/60 px-3 py-2 text-[11px] text-muted">
      <summary className="cursor-pointer select-none">
        {t("importSkippedEntries", { count: entries.length })}
      </summary>
      <ul className="mt-1.5 max-h-40 space-y-0.5 overflow-y-auto font-mono">
        {entries.map((entry) => (
          <li key={entry} className="truncate" title={entry}>
            {entry}
          </li>
        ))}
      </ul>
    </details>
  );
}

export function ImportDialog({
  open: isOpen,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { indexProgress } = useStore();
  const settingsQ = useSettings(isOpen);
  const projectsDir = settingsQ.data?.resolved.claude.projects ?? "";

  const [phase, setPhase] = useState<Phase>("pick");
  const [zipPath, setZipPath] = useState<string | null>(null);
  const [inspection, setInspection] = useState<ImportInspection | null>(null);
  const [rows, setRows] = useState<MappingRow[]>([]);
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [decisions, setDecisions] = useState<Record<string, boolean>>({});
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const busy =
    phase === "inspecting" || phase === "planning" || phase === "applying";

  // 每次打开都从头开始
  useEffect(() => {
    if (isOpen) {
      setPhase("pick");
      setZipPath(null);
      setInspection(null);
      setRows([]);
      setPlan(null);
      setDecisions({});
      setResult(null);
      setError(null);
    }
  }, [isOpen]);

  const conflicts = useMemo(
    () => plan?.sessions.filter((session) => session.action === "conflict") ?? [],
    [plan]
  );
  const others = useMemo(
    () => plan?.sessions.filter((session) => session.action !== "conflict") ?? [],
    [plan]
  );

  if (!isOpen) return null;

  const pickZip = async () => {
    setError(null);
    let selected: string | string[] | null = null;
    try {
      selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Zip", extensions: ["zip"] }],
      });
    } catch (e) {
      setError(errMessage(e));
      return;
    }
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;
    setZipPath(path);
    setPhase("inspecting");
    try {
      const next = await api.inspectSessionImport(path);
      setInspection(next);
      setRows(rowsFromInspection(next));
      setPhase("mapping");
    } catch (e) {
      setError(errMessage(e));
      setPhase("pick");
    }
  };

  const chooseFolder = async (index: number) => {
    let selected: string | string[] | null = null;
    try {
      selected = await open({
        directory: true,
        multiple: false,
        defaultPath: rows[index]?.local || undefined,
      });
    } catch (e) {
      setError(errMessage(e));
      return;
    }
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;
    setRows((current) =>
      current.map((row, i) => (i === index ? { ...row, local: path } : row))
    );
  };

  const setLocal = (index: number, local: string) => {
    setRows((current) =>
      current.map((row, i) => (i === index ? { ...row, local } : row))
    );
  };

  const applyPlan = async (
    path: string,
    mappings: ProjectMapping[],
    chosen: ConflictDecision[]
  ) => {
    setPhase("applying");
    setError(null);
    try {
      const next = await api.applySessionImport(path, mappings, chosen);
      setResult(next);
      setPhase("done");
      await queryClient.invalidateQueries();
    } catch (e) {
      setError(errMessage(e));
      setPhase(plan && plan.counts.conflict > 0 ? "conflicts" : "mapping");
    }
  };

  const startImport = async () => {
    if (!zipPath) return;
    setError(null);
    setPhase("planning");
    const mappings = mappingsFromRows(rows);
    try {
      const next = await api.planSessionImport(zipPath, mappings);
      setPlan(next);
      if (next.counts.conflict > 0) {
        const defaults: Record<string, boolean> = {};
        for (const session of next.sessions) {
          if (session.action === "conflict") defaults[sessionKey(session)] = true;
        }
        setDecisions(defaults);
        setPhase("conflicts");
        return;
      }
      await applyPlan(zipPath, mappings, []);
    } catch (e) {
      setError(errMessage(e));
      setPhase("mapping");
    }
  };

  const confirmConflicts = async () => {
    if (!zipPath || !plan) return;
    const chosen: ConflictDecision[] = conflicts.map((session) => ({
      cloudDir: session.cloudDir,
      sessionId: session.sessionId,
      keepLocal: decisions[sessionKey(session)] ?? true,
    }));
    await applyPlan(zipPath, mappingsFromRows(rows), chosen);
  };

  const openProject = (path: string) => {
    onClose();
    navigate(`/project/${encodePath(path)}`);
  };

  const requestClose = () => {
    if (!busy) onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50" onClick={requestClose} aria-hidden />

      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-dialog-title"
        className="relative flex max-h-[90vh] w-full max-w-2xl flex-col rounded-lg border border-border bg-surface shadow-2xl"
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3.5">
          <h2
            id="import-dialog-title"
            className="flex items-center gap-2 text-sm font-semibold text-foreground"
          >
            <FileArchive size={15} className="text-accent" />
            {t("importTitle")}
          </h2>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={requestClose}
            disabled={busy}
            title={t("close")}
          >
            <X size={16} />
          </Button>
        </div>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {/* ---------- 第一步：选文件 ---------- */}
          {(phase === "pick" || phase === "inspecting") && (
            <>
              <p className="text-xs leading-relaxed text-muted">{t("importIntro")}</p>
              {projectsDir && (
                <p
                  className="truncate text-[11px] text-muted"
                  title={projectsDir}
                >
                  {t("importTargetDir", { path: projectsDir })}
                </p>
              )}
              <div className="flex items-center gap-3">
                <Button
                  onClick={() => void pickZip()}
                  disabled={phase === "inspecting"}
                  data-testid="import-pick-zip"
                >
                  {phase === "inspecting" ? (
                    <Spinner className="border-accent-fg/40 border-t-accent-fg" />
                  ) : (
                    <FileArchive size={14} />
                  )}
                  {phase === "inspecting" ? t("importInspecting") : t("importPickZip")}
                </Button>
                {zipPath && phase === "inspecting" && (
                  <span className="min-w-0 truncate text-xs text-muted" title={zipPath}>
                    {zipPath}
                  </span>
                )}
              </div>
            </>
          )}

          {/* ---------- 第二步：项目映射 ---------- */}
          {(phase === "mapping" || phase === "planning") && inspection && (
            <>
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                <span className="min-w-0 truncate font-medium text-foreground" title={zipPath ?? ""}>
                  {zipPath}
                </span>
                <Badge tone="accent">
                  {t("importSessionsInZip", { count: inspection.sessionCount })}
                </Badge>
              </div>
              {inspection.sessionCount === 0 ? (
                <p className="text-xs text-danger">{t("importNoSessions")}</p>
              ) : (
                <section className="space-y-2">
                  <h3 className="text-xs font-semibold text-foreground">
                    {t("importProjectsHeading")}
                  </h3>
                  <p className="text-[11px] leading-relaxed text-muted">
                    {t("importProjectsHint")}
                  </p>
                  <ul className="space-y-2">
                    {rows.map((row, index) => {
                      const keepOriginal = row.local.trim() === "";
                      return (
                        <li
                          key={row.cloudDir}
                          className="space-y-2 rounded-lg border border-border bg-surface-2/40 p-3"
                          data-testid="import-mapping-row"
                        >
                          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                            <span className="text-muted">{t("importCloudProject")}</span>
                            <span className="min-w-0 truncate font-mono text-foreground" title={row.cloudCwd}>
                              {row.cloudCwd}
                            </span>
                            <Badge tone="muted">
                              {t("importSessionsInZip", { count: row.sessionCount })}
                            </Badge>
                          </div>
                          <div className="flex flex-wrap items-center gap-2">
                            <span className="shrink-0 text-xs text-muted">{t("importLocalPath")}</span>
                            <Input
                              value={row.local}
                              placeholder={t("importLocalPathPlaceholder")}
                              onChange={(event) => setLocal(index, event.target.value)}
                              spellCheck={false}
                              className="h-8 min-w-0 flex-1 text-xs"
                              disabled={phase === "planning"}
                            />
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={() => void chooseFolder(index)}
                              disabled={phase === "planning"}
                            >
                              <FolderSearch size={13} />
                              {t("importChooseFolder")}
                            </Button>
                            {keepOriginal ? (
                              <Badge tone="muted">{t("importKeepOriginal")}</Badge>
                            ) : (
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setLocal(index, "")}
                                disabled={phase === "planning"}
                              >
                                {t("importKeepOriginal")}
                              </Button>
                            )}
                            {row.source && row.local && (
                              <Badge tone={row.source === "remembered" ? "accent" : "outline"}>
                                {row.source === "remembered"
                                  ? t("importSuggestionRemembered")
                                  : t("importSuggestionByName")}
                              </Badge>
                            )}
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                </section>
              )}
              <SkippedEntries entries={inspection.skippedEntries} />
            </>
          )}

          {/* ---------- 冲突处理 ---------- */}
          {phase === "conflicts" && plan && (
            <>
              <p className="text-xs font-medium text-foreground">
                {t("importPlanSummary", {
                  add: plan.counts.add,
                  update: plan.counts.update,
                  skip: plan.counts.skip,
                  conflict: plan.counts.conflict,
                })}
              </p>
              <section className="space-y-2">
                <h3 className="flex items-center gap-1.5 text-xs font-semibold text-warning">
                  <AlertTriangle size={13} />
                  {t("importConflictsTitle", { count: conflicts.length })}
                </h3>
                <p className="text-[11px] leading-relaxed text-muted">{t("importConflictHint")}</p>
                <ul className="space-y-2">
                  {conflicts.map((session) => {
                    const key = sessionKey(session);
                    const keepLocal = decisions[key] ?? true;
                    return (
                      <li
                        key={key}
                        className="space-y-2 rounded-lg border border-warning/40 bg-surface-2/40 p-3"
                        data-testid="import-conflict-row"
                      >
                        <SessionTitle session={session} />
                        <div className="flex flex-wrap gap-x-4 gap-y-1">
                          {session.local && (
                            <SideSummary label={t("importSideLocal")} side={session.local} />
                          )}
                          <SideSummary label={t("importSideImported")} side={session.imported} />
                        </div>
                        <div
                          className="inline-flex items-center rounded-lg border border-border bg-background p-0.5"
                          role="radiogroup"
                        >
                          {[true, false].map((value) => (
                            <button
                              key={String(value)}
                              type="button"
                              role="radio"
                              aria-checked={keepLocal === value}
                              onClick={() =>
                                setDecisions((current) => ({ ...current, [key]: value }))
                              }
                              className={cn(
                                "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                                keepLocal === value
                                  ? "bg-accent text-accent-fg"
                                  : "text-muted hover:text-foreground"
                              )}
                            >
                              {value ? t("importKeepLocal") : t("importOverwriteLocal")}
                            </button>
                          ))}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </section>
              {others.length > 0 && (
                <details className="rounded-lg border border-border px-3 py-2 text-xs">
                  <summary className="cursor-pointer select-none text-muted">
                    {t("importOtherSessions", { count: others.length })}
                  </summary>
                  <ul className="mt-2 space-y-1.5">
                    {others.map((session) => (
                      <li key={sessionKey(session)} className="flex items-start gap-2">
                        <Badge tone={ACTION_TONE[session.action]} className="mt-0.5 shrink-0">
                          {t(ACTION_KEY[session.action])}
                        </Badge>
                        <SessionTitle session={session} />
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              <SkippedEntries entries={plan.skippedEntries} />
            </>
          )}

          {/* ---------- 写入中 ---------- */}
          {phase === "applying" && (
            <div className="flex items-center gap-2 py-6 text-xs text-muted">
              <Spinner />
              {indexProgress ? indexProgressLabel(indexProgress, t) : t("importApplying")}
            </div>
          )}

          {/* ---------- 汇总 ---------- */}
          {phase === "done" && result && (
            <>
              <h3 className="flex items-center gap-1.5 text-xs font-semibold text-success">
                <Check size={14} />
                {t("importDoneTitle")}
              </h3>
              <dl className="grid grid-cols-2 gap-2 sm:grid-cols-5" data-testid="import-summary">
                {(
                  [
                    ["importResultAdded", result.added],
                    ["importResultUpdated", result.updated],
                    ["importResultSkipped", result.skipped],
                    ["importResultKeptLocal", result.keptLocal],
                    ["importResultOverwritten", result.overwritten],
                  ] as [DictKey, number][]
                ).map(([key, value]) => (
                  <div key={key} className="rounded-lg bg-surface-2/60 px-3 py-2">
                    <dt className="text-[11px] text-muted">{t(key)}</dt>
                    <dd className="text-lg font-semibold tabular-nums text-foreground">
                      {formatNumber(value)}
                    </dd>
                  </div>
                ))}
              </dl>
              <p className="text-[11px] text-muted">
                {t("importFilesWritten", { count: formatNumber(result.filesWritten) })}
              </p>
              {result.projects.length > 0 && (
                <ul className="space-y-1">
                  {result.projects.map((project) => (
                    <li key={project.path} className="flex items-center gap-2 text-xs">
                      <span className="min-w-0 flex-1 truncate" title={project.path}>
                        <span className="font-medium text-foreground">{project.name}</span>{" "}
                        <span className="text-muted">{prettyPath(project.path)}</span>
                      </span>
                      <button
                        type="button"
                        onClick={() => openProject(project.path)}
                        className="flex shrink-0 items-center gap-1 text-accent hover:underline"
                      >
                        <FolderOpen size={12} />
                        {t("importOpenProject")}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <SkippedEntries entries={result.skippedEntries} />
            </>
          )}

          {error && (
            <p className="text-xs text-danger" role="alert">
              {t("importFailed", { error })}
            </p>
          )}
        </div>

        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border px-5 py-3">
          {(phase === "mapping" || phase === "planning") && (
            <>
              <Button variant="outline" size="sm" onClick={() => setPhase("pick")} disabled={busy}>
                {t("importBack")}
              </Button>
              <Button
                size="sm"
                onClick={() => void startImport()}
                disabled={busy || !inspection || inspection.sessionCount === 0}
                data-testid="import-start"
              >
                {phase === "planning" && (
                  <Spinner className="border-accent-fg/40 border-t-accent-fg" />
                )}
                {phase === "planning" ? t("importPlanning") : t("importStart")}
              </Button>
            </>
          )}
          {phase === "conflicts" && (
            <>
              <Button variant="outline" size="sm" onClick={() => setPhase("mapping")}>
                {t("importCancel")}
              </Button>
              <Button size="sm" onClick={() => void confirmConflicts()} data-testid="import-confirm">
                {t("importConfirmWrite")}
              </Button>
            </>
          )}
          {phase === "done" && (
            <Button size="sm" onClick={onClose}>
              {t("importDone")}
            </Button>
          )}
          {(phase === "pick" || phase === "inspecting") && (
            <Button variant="outline" size="sm" onClick={requestClose} disabled={busy}>
              {t("close")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
