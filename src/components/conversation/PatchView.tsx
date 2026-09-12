import { cn } from "@/lib/utils";

/** Codex apply_patch 补丁文本：按行着色（+ 新增、- 删除、*** / @@ 元信息） */
export function PatchView({ patch }: { patch: string }) {
  const lines = patch.replace(/\n$/, "").split("\n");
  return (
    <pre className="code-surface m-0 overflow-x-auto text-[11.5px] leading-[1.55]">
      <code className="block min-w-full">
        {lines.map((line, index) => {
          const meta = line.startsWith("***") || line.startsWith("@@");
          const add = !meta && line.startsWith("+");
          const del = !meta && line.startsWith("-");
          return (
            <span
              key={index}
              className={cn(
                "block whitespace-pre px-3",
                meta && "font-medium text-accent",
                add && "diff-line-add",
                del && "diff-line-del"
              )}
            >
              {line}
            </span>
          );
        })}
      </code>
    </pre>
  );
}

/** apply_patch 补丁里涉及的文件名，用于折叠块的摘要 */
export function patchFiles(patch: string): string[] {
  const files: string[] = [];
  for (const line of patch.split("\n")) {
    const match = line.match(/^\*\*\* (?:Update|Add|Delete) File: (.+)$/);
    if (match) files.push(match[1].trim());
  }
  return files;
}
