// 设置弹窗：分别配置 Claude Code 与 Codex 的本地数据目录。

import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Trash2, X } from "lucide-react";
import { useSettings } from "@/hooks/queries";
import { api, errMessage } from "@/lib/api";
import { useT } from "@/i18n";
import { useStore } from "@/store";
import type { SettingsInput } from "@/lib/types";
import { Badge, Button, Input, Spinner } from "@/components/ui";
import { indexProgressLabel } from "./IndexProgress";

const EMPTY_FORM: SettingsInput = {
  claudeDataDir: "",
  codexDataDir: "",
  historyFile: "",
  projectsDir: "",
  sessionsDir: "",
  importPathMappings: {},
};

function sameMappings(a: Record<string, string>, b: Record<string, string>) {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  return (
    keysA.length === keysB.length && keysA.every((key) => a[key] === b[key])
  );
}

function ResolvedRow({
  label,
  path,
  exists,
}: {
  label: string;
  path: string;
  exists: boolean;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="w-24 shrink-0 text-muted">{label}</span>
      <span
        className="min-w-0 flex-1 truncate text-foreground"
        title={path}
      >
        {path}
      </span>
      <Badge tone={exists ? "success" : "warning"}>
        {exists ? t("exists") : t("notExists")}
      </Badge>
    </div>
  );
}

function FormField({
  label,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  value: string;
  placeholder?: string;
  onChange: (e: ChangeEvent<HTMLInputElement>) => void;
}) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted">{label}</span>
      <Input
        value={value}
        placeholder={placeholder}
        onChange={onChange}
        spellCheck={false}
      />
    </label>
  );
}

export function SettingsDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const t = useT();
  const { refreshing, refreshIndex, indexProgress } = useStore();
  const settingsQ = useSettings(open);
  const data = settingsQ.data;

  const [form, setForm] = useState<SettingsInput>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedMsg, setSavedMsg] = useState(false);

  // 加载/保存成功后，把表单同步为后端当前值
  useEffect(() => {
    if (data) {
      setForm({
        claudeDataDir: data.claudeDataDir,
        codexDataDir: data.codexDataDir,
        historyFile: data.historyFile,
        projectsDir: data.projectsDir,
        sessionsDir: data.sessionsDir,
        importPathMappings: { ...(data.importPathMappings ?? {}) },
      });
    }
  }, [data]);

  // 每次打开时清掉上次的提示
  useEffect(() => {
    if (open) {
      setSaveError(null);
      setSavedMsg(false);
    }
  }, [open]);

  const dirty = useMemo(() => {
    if (!data) return false;
    return (
      form.claudeDataDir !== data.claudeDataDir ||
      form.codexDataDir !== data.codexDataDir ||
      form.historyFile !== data.historyFile ||
      form.projectsDir !== data.projectsDir ||
      form.sessionsDir !== data.sessionsDir ||
      !sameMappings(form.importPathMappings, data.importPathMappings ?? {})
    );
  }, [form, data]);

  if (!open) return null;

  const setField =
    (key: Exclude<keyof SettingsInput, "importPathMappings">) =>
    (e: ChangeEvent<HTMLInputElement>) => {
      setSavedMsg(false);
      setForm((f) => ({ ...f, [key]: e.target.value }));
    };

  const removeMapping = (cloud: string) => {
    setSavedMsg(false);
    setForm((f) => {
      const next = { ...f.importPathMappings };
      delete next[cloud];
      return { ...f, importPathMappings: next };
    });
  };
  const mappingEntries = Object.entries(form.importPathMappings);

  const handleSave = async () => {
    if (!dirty || saving) return;
    setSaving(true);
    setSaveError(null);
    setSavedMsg(false);
    try {
      const next = await api.setSettings(form);
      // 先用返回值刷新 resolved 显示，再全量失效让索引按新数据源懒重建
      queryClient.setQueryData(["settings"], next);
      await queryClient.invalidateQueries();
      setSavedMsg(true);
    } catch (e) {
      setSaveError(errMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* 遮罩 */}
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />

      {/* 卡片 */}
      <div className="relative max-h-[90vh] w-full max-w-xl overflow-y-auto rounded-lg border border-border bg-surface shadow-2xl">
        <div className="flex items-center justify-between border-b border-border px-5 py-3.5">
          <h2 className="text-sm font-semibold text-foreground">
            {t("settingsTitle")}
          </h2>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onClose}
            title={t("close")}
          >
            <X size={16} />
          </Button>
        </div>

        <div className="space-y-4 px-5 py-4">
          {settingsQ.isLoading ? (
            <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted">
              <Spinner /> {t("loadingSettings")}
            </div>
          ) : settingsQ.isError ? (
            <p className="py-6 text-center text-xs text-danger">
              {t("loadSettingsFailed", {
                error: errMessage(settingsQ.error),
              })}
            </p>
          ) : (
            <>
              <section className="space-y-3 rounded-lg border border-border p-3">
                <h3 className="text-xs font-semibold text-foreground">
                  {t("claudeDataSource")}
                </h3>
                <FormField
                  label={t("claudeDataDirLabel")}
                  value={form.claudeDataDir}
                  placeholder={t("claudeDataDirPlaceholder")}
                  onChange={setField("claudeDataDir")}
                />

                <details className="rounded-lg border border-border">
                  <summary className="cursor-pointer select-none px-3 py-2 text-xs font-medium text-muted">
                    {t("advancedOverrides")}
                  </summary>
                  <div className="space-y-3 px-3 pb-3 pt-1">
                    <FormField
                      label={t("historyFileLabel")}
                      value={form.historyFile}
                      placeholder={t("overridePlaceholder")}
                      onChange={setField("historyFile")}
                    />
                    <FormField
                      label={t("projectsDirLabel")}
                      value={form.projectsDir}
                      placeholder={t("overridePlaceholder")}
                      onChange={setField("projectsDir")}
                    />
                    <FormField
                      label={t("sessionsDirLabel")}
                      value={form.sessionsDir}
                      placeholder={t("overridePlaceholder")}
                      onChange={setField("sessionsDir")}
                    />
                  </div>
                </details>
              </section>

              <section className="space-y-3 rounded-lg border border-border p-3">
                <h3 className="text-xs font-semibold text-foreground">
                  {t("codexDataSource")}
                </h3>
                <FormField
                  label={t("codexDataDirLabel")}
                  value={form.codexDataDir}
                  placeholder={t("codexDataDirPlaceholder")}
                  onChange={setField("codexDataDir")}
                />
              </section>

              <section className="space-y-2 rounded-lg border border-border p-3">
                <h3 className="text-xs font-semibold text-foreground">
                  {t("importMappingsSection")}
                </h3>
                <p className="text-[11px] leading-relaxed text-muted">
                  {t("importMappingsHint")}
                </p>
                {mappingEntries.length === 0 ? (
                  <p className="text-[11px] text-muted">{t("importMappingsEmpty")}</p>
                ) : (
                  <ul className="divide-y divide-border rounded-lg bg-surface-2/60">
                    {mappingEntries.map(([cloud, local]) => (
                      <li
                        key={cloud}
                        className="flex items-center gap-2 px-3 py-1.5 text-xs"
                      >
                        <span className="min-w-0 flex-1 truncate" title={cloud}>
                          {cloud}
                        </span>
                        <span className="shrink-0 text-muted">→</span>
                        <span
                          className="min-w-0 flex-1 truncate text-foreground"
                          title={local}
                        >
                          {local}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          onClick={() => removeMapping(cloud)}
                          title={t("importMappingRemove")}
                        >
                          <Trash2 size={13} />
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              <section className="space-y-3 rounded-lg border border-border p-3">
                <h3 className="text-xs font-semibold text-foreground">
                  {t("indexMaintenance")}
                </h3>
                <div className="grid gap-2 sm:grid-cols-2">
                  <div className="flex flex-col rounded-lg bg-surface-2/60 p-3">
                    <div className="text-xs font-medium text-foreground">
                      {t("incrementalRefresh")}
                    </div>
                    <p className="mt-1 flex-1 text-[11px] leading-relaxed text-muted">
                      {t("incrementalRefreshHint")}
                    </p>
                    <Button
                      variant="outline"
                      size="sm"
                      className="mt-2 self-start"
                      disabled={refreshing}
                      onClick={() => void refreshIndex(false)}
                    >
                      {t("incrementalRefresh")}
                    </Button>
                  </div>
                  <div className="flex flex-col rounded-lg bg-surface-2/60 p-3">
                    <div className="text-xs font-medium text-foreground">
                      {t("fullRebuild")}
                    </div>
                    <p className="mt-1 flex-1 text-[11px] leading-relaxed text-muted">
                      {t("fullRebuildHint")}
                    </p>
                    <Button
                      variant="outline"
                      size="sm"
                      className="mt-2 self-start"
                      disabled={refreshing}
                      onClick={() => void refreshIndex(true)}
                    >
                      {t("fullRebuild")}
                    </Button>
                  </div>
                </div>
                {(refreshing || indexProgress) && (
                  <p className="flex items-center gap-2 text-[11px] text-muted">
                    <Spinner />
                    {indexProgress
                      ? indexProgressLabel(indexProgress, t)
                      : t("indexingScanning")}
                  </p>
                )}
              </section>

              {data && (
                <div className="space-y-3 rounded-lg bg-surface-2/60 p-3">
                  <div className="text-xs font-medium text-foreground">
                    {t("resolvedPaths")}
                  </div>
                  <div className="space-y-2">
                    <div className="text-[11px] font-semibold text-muted">
                      {t("claudeDataSource")}
                    </div>
                    <ResolvedRow
                      label={t("historyFileShort")}
                      path={data.resolved.claude.history}
                      exists={data.resolved.claude.historyExists}
                    />
                    <ResolvedRow
                      label={t("projectsDirShort")}
                      path={data.resolved.claude.projects}
                      exists={data.resolved.claude.projectsExists}
                    />
                    <ResolvedRow
                      label={t("sessionsDirShort")}
                      path={data.resolved.claude.sessions}
                      exists={data.resolved.claude.sessionsExists}
                    />
                  </div>
                  <div className="space-y-2 border-t border-border pt-3">
                    <div className="text-[11px] font-semibold text-muted">
                      {t("codexDataSource")}
                    </div>
                    <ResolvedRow
                      label={t("rootDirShort")}
                      path={data.resolved.codex.root}
                      exists={data.resolved.codex.rootExists}
                    />
                    <ResolvedRow
                      label={t("historyFileShort")}
                      path={data.resolved.codex.history}
                      exists={data.resolved.codex.historyExists}
                    />
                    <ResolvedRow
                      label={t("sessionsDirShort")}
                      path={data.resolved.codex.sessions}
                      exists={data.resolved.codex.sessionsExists}
                    />
                    <ResolvedRow
                      label={t("archivedSessionsDirShort")}
                      path={data.resolved.codex.archivedSessions}
                      exists={data.resolved.codex.archivedSessionsExists}
                    />
                  </div>
                </div>
              )}

              {saveError && (
                <p className="text-xs text-danger">
                  {t("saveFailed", { error: saveError })}
                </p>
              )}
              {savedMsg && (
                <p className="text-xs text-success">{t("savedMessage")}</p>
              )}
            </>
          )}
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border px-5 py-3">
          <span
            className="min-w-0 flex-1 truncate text-[11px] text-muted"
            title={data?.configPath}
          >
            {data ? t("configFileLabel", { path: data.configPath }) : ""}
          </span>
          <div className="flex shrink-0 items-center gap-2">
            <Button variant="outline" size="sm" onClick={onClose}>
              {t("close")}
            </Button>
            <Button
              size="sm"
              onClick={handleSave}
              disabled={!dirty || saving || !data}
            >
              {saving && (
                <Spinner className="border-accent-fg/40 border-t-accent-fg" />
              )}
              {saving ? t("saving") : t("save")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
