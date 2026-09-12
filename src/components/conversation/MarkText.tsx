import { Fragment } from "react";
import { splitByRegex } from "@/lib/textMatch";

/** 原文模式下的关键词高亮 */
export function MarkText({
  text,
  regex,
}: {
  text: string;
  regex: RegExp | null;
}) {
  if (!regex) return <>{text}</>;
  return (
    <>
      {splitByRegex(text, regex).map((part, index) =>
        part.hit ? (
          <mark key={index} className="find-hit">
            {part.text}
          </mark>
        ) : (
          <Fragment key={index}>{part.text}</Fragment>
        )
      )}
    </>
  );
}
