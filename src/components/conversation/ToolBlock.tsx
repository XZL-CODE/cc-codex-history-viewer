// 工具调用与结果的智能渲染：常见工具按语义展示（命令、文件、diff、补丁、待办…），
// 未识别的工具回退为键值 + JSON。所有正文只在折叠块展开时渲染。

import { useState, type ReactNode } from "react";
import {
  CheckCircle2,
  Circle,
  CircleDot,
  CornerDownRight,
  FileText,
  Paperclip,
  Wrench,
} from "lucide-react";
import type { ContentBlock, PersistedOutputText } from "@/lib/types";
import { api, errMessage } from "@/lib/api";
import { useT } from "@/i18n";
import { cn, formatBytes, pathBasename } from "@/lib/utils";
import { Collapsible } from "./Collapsible";
import { DiffView } from "./DiffView";
import { fencedCode, Markdown } from "./Markdown";
import { MarkText } from "./MarkText";
import { PatchView, patchFiles } from "./PatchView";

type Input = Record<string, unknown>;

function asRecord(value: unknown): Input | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Input)
    : null;
}

function str(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function firstLine(text: string, max = 120): string {
  const line = text.trim().split("\n")[0] ?? "";
  return line.length > max ? `${line.slice(0, max)}…` : line;
}

const LANG_BY_EXT: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  rs: "rust",
  py: "python",
  go: "go",
  java: "java",
  kt: "kotlin",
  swift: "swift",
  c: "c",
  h: "c",
  cc: "cpp",
  cpp: "cpp",
  hpp: "cpp",
  cs: "csharp",
  rb: "ruby",
  php: "php",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  fish: "bash",
  json: "json",
  jsonl: "json",
  yaml: "yaml",
  yml: "yaml",
  toml: "ini",
  ini: "ini",
  md: "markdown",
  mdx: "markdown",
  html: "xml",
  htm: "xml",
  xml: "xml",
  svg: "xml",
  css: "css",
  scss: "scss",
  less: "less",
  sql: "sql",
  graphql: "graphql",
  lua: "lua",
  r: "r",
  vue: "xml",
  dockerfile: "dockerfile",
  makefile: "makefile",
  diff: "diff",
  patch: "diff",
};

function langFor(path: string): string {
  const base = pathBasename(path).toLowerCase();
  if (base === "dockerfile") return "dockerfile";
  if (base === "makefile") return "makefile";
  const ext = base.includes(".") ? base.slice(base.lastIndexOf(".") + 1) : "";
  return LANG_BY_EXT[ext] ?? "";
}

function commandText(input: Input): string | null {
  const raw = input.command ?? input.cmd ?? input.script;
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) {
    const parts = raw.filter((item): item is string => typeof item === "string");
    // Codex 常见形状 ["bash","-lc","..."]：直接展示真正的脚本
    if (parts.length >= 3 && /^(-l?c|-c)$/.test(parts[1])) return parts.slice(2).join(" ");
    return parts.join(" ");
  }
  return null;
}

function filePath(input: Input): string | null {
  return (
    str(input.file_path) ??
    str(input.path) ??
    str(input.filename) ??
    str(input.notebook_path) ??
    null
  );
}

function patchText(name: string, input: Input | null, raw: unknown): string | null {
  const candidates = [
    input?.patch,
    input?.input,
    input?.diff,
    typeof raw === "string" ? raw : null,
  ];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.includes("*** Begin Patch")) {
      return candidate;
    }
  }
  return /apply_patch/i.test(name) && input ? str(input.input) : null;
}

const SHELL_TOOLS = /^(bash|shell|exec_command|local_shell|shell_command|run_command|exec|container\.exec|powershell|zsh|sh)$/i;

function KeyValue({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 px-3 py-2 text-xs">
      {rows.map(([key, value]) => (
        <div key={key} className="contents">
          <dt className="text-muted">{key}</dt>
          <dd className="min-w-0 break-all font-mono text-[11.5px] text-foreground">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function RawJson({ value }: { value: unknown }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="border-t border-border/60 px-3 py-1.5">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="text-[11px] text-muted hover:text-foreground"
      >
        {open ? t("hideRawJson") : t("showRawJson")}
      </button>
      {open && (
        <pre className="code-surface mt-1.5 overflow-x-auto whitespace-pre-wrap break-words p-2 text-[11px]">
          {JSON.stringify(value ?? {}, null, 2)}
        </pre>
      )}
    </div>
  );
}

function CodeFence({ code, lang }: { code: string; lang: string }) {
  return (
    <div className="px-3 py-2">
      <Markdown text={fencedCode(code, lang)} />
    </div>
  );
}

function PlainText({ text }: { text: string }) {
  return (
    <pre className="whitespace-pre-wrap break-words px-3 py-2 text-[11.5px] leading-relaxed text-foreground">
      {text}
    </pre>
  );
}

interface ToolView {
  hint: string;
  body: ReactNode;
}

function describeTool(
  name: string,
  raw: unknown,
  t: ReturnType<typeof useT>
): ToolView {
  const input = asRecord(raw);
  const patch = patchText(name, input, raw);
  if (patch) {
    const files = patchFiles(patch);
    return {
      hint: files.length > 0 ? files.join(", ") : t("toolPatchHint"),
      body: <PatchView patch={patch} />,
    };
  }
  if (!input) {
    return {
      hint: typeof raw === "string" ? firstLine(raw) : "",
      body: (
        <PlainText
          text={typeof raw === "string" ? raw : JSON.stringify(raw ?? {}, null, 2)}
        />
      ),
    };
  }

  const command = commandText(input);
  if (command && (SHELL_TOOLS.test(name) || input.command !== undefined || input.cmd !== undefined)) {
    const description = str(input.description);
    return {
      hint: firstLine(command),
      body: (
        <>
          {description && (
            <p className="px-3 pt-2 text-[11.5px] text-muted">{description}</p>
          )}
          <CodeFence code={command} lang="bash" />
        </>
      ),
    };
  }

  const path = filePath(input);

  if (/^(edit|str_replace_editor|str_replace_based_edit_tool)$/i.test(name) && path) {
    const oldText = str(input.old_string) ?? str(input.old_str) ?? "";
    const newText = str(input.new_string) ?? str(input.new_str) ?? "";
    return {
      hint: path,
      body: (
        <>
          <KeyValue
            rows={[
              [t("toolFilePath"), path],
              ...(input.replace_all === true
                ? [[t("toolEditReplaceAll"), "true"] as [string, ReactNode]]
                : []),
            ]}
          />
          <DiffView oldText={oldText} newText={newText} />
        </>
      ),
    };
  }

  if (/^multiedit$/i.test(name) && path && Array.isArray(input.edits)) {
    const edits = input.edits.filter(asRecord).map(asRecord) as Input[];
    return {
      hint: `${path} · ${t("toolEditCount", { count: edits.length })}`,
      body: (
        <>
          <KeyValue rows={[[t("toolFilePath"), path]]} />
          {edits.map((edit, index) => (
            <div key={index} className="border-t border-border/60">
              <DiffView
                oldText={str(edit.old_string) ?? ""}
                newText={str(edit.new_string) ?? ""}
              />
            </div>
          ))}
        </>
      ),
    };
  }

  if (/^(write|create_file|write_file)$/i.test(name) && path) {
    const content = str(input.content) ?? str(input.file_text) ?? "";
    return {
      hint: path,
      body: (
        <>
          <KeyValue rows={[[t("toolFilePath"), path]]} />
          {content && <CodeFence code={content} lang={langFor(path)} />}
        </>
      ),
    };
  }

  if (/^(read|read_file|view|cat|open_file)$/i.test(name) && path) {
    const rows: [string, ReactNode][] = [[t("toolFilePath"), path]];
    if (input.offset !== undefined) rows.push(["offset", String(input.offset)]);
    if (input.limit !== undefined) rows.push(["limit", String(input.limit)]);
    return { hint: path, body: <KeyValue rows={rows} /> };
  }

  if (/^(glob|grep|search|rg|find|list_dir|ls)$/i.test(name)) {
    const pattern = str(input.pattern) ?? str(input.query) ?? str(input.glob);
    const rows: [string, ReactNode][] = [];
    if (pattern) rows.push([t("toolPattern"), pattern]);
    if (path) rows.push([t("toolFilePath"), path]);
    for (const [key, value] of Object.entries(input)) {
      if (["pattern", "query", "glob", "path", "file_path"].includes(key)) continue;
      if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
        rows.push([key, String(value)]);
      }
    }
    return {
      hint: [pattern, path ? pathBasename(path) : null].filter(Boolean).join(" · "),
      body: <KeyValue rows={rows} />,
    };
  }

  if (/^(task|agent|spawn_agent|subagent)$/i.test(name)) {
    const description = str(input.description);
    const prompt = str(input.prompt) ?? str(input.instructions);
    const type = str(input.subagent_type) ?? str(input.agent_type);
    return {
      hint: [type, description].filter(Boolean).join(" · "),
      body: (
        <>
          {(type || description) && (
            <KeyValue
              rows={[
                ...(type ? [["type", type] as [string, ReactNode]] : []),
                ...(description
                  ? [[t("toolDescription"), description] as [string, ReactNode]]
                  : []),
              ]}
            />
          )}
          {prompt && <PlainText text={prompt} />}
        </>
      ),
    };
  }

  if (/^(webfetch|web_fetch|fetch|open_url)$/i.test(name) || str(input.url)) {
    const url = str(input.url) ?? "";
    const prompt = str(input.prompt);
    return {
      hint: url,
      body: (
        <>
          <KeyValue rows={[[t("toolUrl"), url]]} />
          {prompt && <PlainText text={prompt} />}
        </>
      ),
    };
  }

  if (/^(websearch|web_search|search_web)$/i.test(name) && str(input.query)) {
    return {
      hint: str(input.query) ?? "",
      body: <KeyValue rows={[[t("toolQuery"), str(input.query) ?? ""]]} />,
    };
  }

  if (/^(todowrite|update_plan|todo)$/i.test(name)) {
    const items = (Array.isArray(input.todos) ? input.todos : Array.isArray(input.plan) ? input.plan : [])
      .map(asRecord)
      .filter((item): item is Input => item !== null);
    const done = items.filter((item) => item.status === "completed").length;
    return {
      hint: t("toolTodoProgress", { done, total: items.length }),
      body: (
        <ul className="space-y-1 px-3 py-2 text-xs">
          {items.map((item, index) => {
            const status = String(item.status ?? "pending");
            const Icon =
              status === "completed"
                ? CheckCircle2
                : status === "in_progress"
                  ? CircleDot
                  : Circle;
            return (
              <li key={index} className="flex items-start gap-1.5">
                <Icon
                  size={13}
                  className={cn(
                    "mt-0.5 shrink-0",
                    status === "completed"
                      ? "text-success"
                      : status === "in_progress"
                        ? "text-accent"
                        : "text-muted"
                  )}
                />
                <span className={cn(status === "completed" && "text-muted line-through")}>
                  {str(item.content) ?? str(item.step) ?? JSON.stringify(item)}
                </span>
              </li>
            );
          })}
        </ul>
      ),
    };
  }

  // 通用回退：标量字段列表 + 原始 JSON
  const scalarRows: [string, ReactNode][] = Object.entries(input)
    .filter(([, value]) => ["string", "number", "boolean"].includes(typeof value))
    .map(([key, value]) => [key, String(value)]);
  const hintSource = path ?? str(input.query) ?? str(input.name) ?? scalarRows[0]?.[1];
  return {
    hint: typeof hintSource === "string" ? firstLine(hintSource, 80) : "",
    body:
      scalarRows.length > 0 ? (
        <KeyValue rows={scalarRows} />
      ) : (
        <PlainText text={JSON.stringify(input, null, 2)} />
      ),
  };
}

export function ToolUseBlock({
  block,
  regex,
}: {
  block: ContentBlock;
  regex: RegExp | null;
}) {
  const t = useT();
  const name = block.toolName ?? "tool";
  const view = describeTool(name, block.toolInput, t);
  return (
    <Collapsible
      summary={
        <>
          <Wrench size={12} className="shrink-0 text-accent" />
          <span className="shrink-0 text-foreground">
            <MarkText text={name} regex={regex} />
          </span>
          {view.hint && (
            <span className="min-w-0 truncate font-normal text-muted">
              <MarkText text={view.hint} regex={regex} />
            </span>
          )}
        </>
      }
    >
      {view.body}
      <RawJson value={block.toolInput} />
    </Collapsible>
  );
}

/** 后端按需读取持久化输出的上限（与 commands.rs 的 DISPLAY_PERSISTED_MAX_BYTES 一致） */
const PERSISTED_DISPLAY_LIMIT = 4 * 1024 * 1024;

/**
 * 持久化的超大工具输出：正文默认是 Claude Code 留下的预览，本机能找到文件时可以
 * 按需读取完整输出替换显示；只在展示层处理，不改写会话文件。
 */
function PersistedOutputControls({
  block,
  full,
  onLoaded,
  onReset,
}: {
  block: ContentBlock;
  full: PersistedOutputText | null;
  onLoaded: (value: PersistedOutputText) => void;
  onReset: () => void;
}) {
  const t = useT();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const persisted = block.persistedOutput;
  if (!persisted) return null;

  const load = async () => {
    if (loading) return;
    setLoading(true);
    setError(null);
    try {
      onLoaded(await api.readPersistedOutput(persisted.path));
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border/60 px-3 py-1.5 text-[11px]">
      {full ? (
        <>
          <span className="min-w-0 truncate text-muted" title={persisted.path}>
            <FileText size={11} className="mr-1 inline-block align-[-1px]" />
            {t("persistedOutputShown", { path: persisted.path })}
          </span>
          <button
            type="button"
            onClick={onReset}
            className="shrink-0 text-accent hover:underline"
          >
            {t("persistedOutputPreview")}
          </button>
          {full.truncated && (
            <span className="text-warning">
              {t("persistedOutputTruncated", {
                size: formatBytes(PERSISTED_DISPLAY_LIMIT),
              })}
            </span>
          )}
        </>
      ) : (
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading}
          className="flex items-center gap-1 font-medium text-accent hover:underline disabled:opacity-60"
        >
          <FileText size={11} />
          {loading
            ? t("persistedOutputLoading")
            : t("persistedOutputView", { size: formatBytes(persisted.size) })}
        </button>
      )}
      {error && (
        <span className="text-danger">
          {t("persistedOutputFailed", { error })}
        </span>
      )}
    </div>
  );
}

export function ToolResultBlock({
  block,
  regex,
}: {
  block: ContentBlock;
  regex: RegExp | null;
}) {
  const t = useT();
  const [full, setFull] = useState<PersistedOutputText | null>(null);
  const text = full ? full.text : (block.text ?? "");
  const preview = firstLine(block.text ?? "", 90);
  return (
    <Collapsible
      summary={
        <>
          <CornerDownRight size={12} className="shrink-0 text-muted" />
          <span className="shrink-0 text-foreground">
            {block.toolName
              ? t("toolResultOf", { name: block.toolName })
              : t("toolResultLabel")}
          </span>
          {preview && (
            <span className="min-w-0 truncate font-normal text-muted">
              <MarkText text={preview} regex={regex} />
            </span>
          )}
          {block.persistedOutput && (
            <FileText size={11} className="shrink-0 text-accent" aria-hidden />
          )}
        </>
      }
    >
      <PersistedOutputControls
        block={block}
        full={full}
        onLoaded={setFull}
        onReset={() => setFull(null)}
      />
      <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11.5px] leading-relaxed text-foreground">
        <MarkText text={text} regex={regex} />
      </pre>
      {block.truncated && !full && (
        <p className="px-3 pb-2 text-[11px] text-warning">{t("blockTruncatedNote")}</p>
      )}
    </Collapsible>
  );
}

/** @ 引用或上传的文件：折叠块显示「附件：文件名」，展开看内容 */
export function AttachmentBlock({
  block,
  regex,
}: {
  block: ContentBlock;
  regex: RegExp | null;
}) {
  const t = useT();
  const text = block.text ?? "";
  const name = block.toolName ? pathBasename(block.toolName) : "";
  return (
    <Collapsible
      summary={
        <>
          <Paperclip size={12} className="shrink-0 text-muted" />
          <span className="shrink-0 text-foreground">{t("attachmentLabel")}</span>
          {name && (
            <span
              className="min-w-0 truncate font-normal text-muted"
              title={block.toolName ?? undefined}
            >
              <MarkText text={name} regex={regex} />
            </span>
          )}
        </>
      }
    >
      <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11.5px] leading-relaxed text-foreground">
        <MarkText text={text} regex={regex} />
      </pre>
      {block.truncated && (
        <p className="px-3 pb-2 text-[11px] text-warning">{t("blockTruncatedNote")}</p>
      )}
    </Collapsible>
  );
}
