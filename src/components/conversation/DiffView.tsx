import { useMemo } from "react";
import { diffLines } from "diff";
import { cn } from "@/lib/utils";

/** Edit 工具的 old_string → new_string 行级 diff */
export function DiffView({
  oldText,
  newText,
}: {
  oldText: string;
  newText: string;
}) {
  const rows = useMemo(() => {
    const parts = diffLines(oldText, newText);
    const out: { sign: "+" | "-" | " "; text: string }[] = [];
    for (const part of parts) {
      const sign = part.added ? "+" : part.removed ? "-" : " ";
      const lines = part.value.replace(/\n$/, "").split("\n");
      for (const line of lines) out.push({ sign, text: line });
    }
    return out;
  }, [oldText, newText]);

  return (
    <pre className="code-surface m-0 overflow-x-auto text-[11.5px] leading-[1.55]">
      <code className="block min-w-full">
        {rows.map((row, index) => (
          <span
            key={index}
            className={cn(
              "block whitespace-pre px-3",
              row.sign === "+" && "diff-line-add",
              row.sign === "-" && "diff-line-del"
            )}
          >
            <span className="mr-2 inline-block w-3 select-none text-muted">
              {row.sign}
            </span>
            {row.text}
          </span>
        ))}
      </code>
    </pre>
  );
}
