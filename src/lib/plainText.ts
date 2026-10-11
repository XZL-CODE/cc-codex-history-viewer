// 标题、大纲与折叠预览用的纯文本摘要。只用于一两行的展示：消息正文、复制与搜索仍用原文。

/** Codex 注入在提问前的上下文包装标题，真正的提问在最后一个「## My request…:」之后 */
const CONTEXT_WRAPPER = /^#\s+(Context from my IDE setup|Files mentioned by the user):?$/;
const REQUEST_HEADING = /^##\s+My request(?: for Codex)?:?$/;
const FENCE_LINE = /^\s*(```|~~~)/;
const TABLE_DIVIDER = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;
/** 行首的标题、引用、列表与任务框标记 */
const LINE_MARKER = /^\s{0,3}(#{1,6}\s+|>\s?|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+)/;

/** 去掉 Codex 的上下文包装，只留真正的提问；不是包装时原样返回 */
export function requestText(text: string): string {
  const lines = text.split("\n");
  const first = lines.find((line) => line.trim() !== "")?.trim() ?? "";
  if (!CONTEXT_WRAPPER.test(first)) return text;
  let request = -1;
  lines.forEach((line, index) => {
    if (REQUEST_HEADING.test(line.trim())) request = index;
  });
  return request >= 0 ? lines.slice(request + 1).join("\n") : text;
}

/** 去掉一行里成对的行内标记：图片与链接只留文字，粗体、删除线、行内代码只留内容 */
function stripInline(line: string): string {
  return line
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__)(.+?)\1/g, "$2")
    .replace(/~~(.+?)~~/g, "$1")
    .replace(/`([^`]+)`/g, "$1");
}

/**
 * 单行纯文本摘要：跳过上下文包装，去掉围栏行、表格分隔行、行首标记与成对的行内标记，
 * 表格行的竖线换成「·」，空白折叠成一个空格。单个 `*` / `_` 常见于路径与通配符，保留不动。
 */
export function plainSummary(text: string): string {
  return requestText(text)
    .split("\n")
    .filter((line) => !FENCE_LINE.test(line) && !TABLE_DIVIDER.test(line))
    .map((line) => {
      let out = line.replace(LINE_MARKER, "");
      const row = out.trim();
      if (row.startsWith("|") && row.endsWith("|") && row.length > 1) {
        out = row.slice(1, -1).split("|").map((cell) => cell.trim()).join(" · ");
      }
      return stripInline(out);
    })
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

/** 按字符（码位）截断，超出时加省略号；不会把代理对或表情切成半个 */
export function clipChars(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length > max ? `${chars.slice(0, max).join("").trimEnd()}…` : text;
}
