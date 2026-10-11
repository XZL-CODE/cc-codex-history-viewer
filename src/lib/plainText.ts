// 标题、大纲与折叠预览用的纯文本摘要。只用于一两行的展示：消息正文、复制与搜索仍用原文。

/** Codex 注入在提问前的上下文包装标题，真正的提问在最后一个「## My request…:」之后 */
const CONTEXT_WRAPPER = /^#\s+(Context from my IDE setup|Files mentioned by the user):?$/;
const REQUEST_HEADING = /^##\s+My request(?: for Codex)?:?$/;
const FENCE_LINE = /^\s*(```|~~~)/;
const TABLE_DIVIDER = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;
/** 行首的标题、引用、列表与任务框标记 */
const LINE_MARKER = /^\s{0,3}(#{1,6}\s+|>\s?|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+)/;
/** 行内代码：成对的反引号串 */
const CODE_SPAN = /(`+)([\s\S]*?[^`])\1(?!`)/g;

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

/**
 * 去掉代码之外成对的行内标记：图片与链接只留文字，粗体、删除线只留内容。
 * 标记两侧要像 Markdown 那样「贴着文字、不贴着单词」，`2**10`、`src/**\/*.ts`、`fns[0](x)`
 * 这类代码写法保持原样；`__` 在标识符里太常见（`__init__`），一律不当作粗体。
 */
function stripMarks(text: string): string {
  return text
    .replace(/(^|[^\w\]!])!\[([^\]]*)\]\([^)\s]*\)/g, "$1$2")
    .replace(/(^|[^\w\]!])\[([^\]]+)\]\([^)\s]*\)/g, "$1$2")
    .replace(/(^|[^\w/*])\*\*([^\s*](?:.*?[^\s*])?)\*\*(?![\w/*])/g, "$1$2")
    .replace(/(^|[^\w~])~~([^\s~](?:.*?[^\s~])?)~~(?![\w~])/g, "$1$2");
}

/** 行内代码原样保留（只去掉反引号），其余部分去掉标记 */
function stripInline(line: string): string {
  let out = "";
  let last = 0;
  for (const match of line.matchAll(CODE_SPAN)) {
    const start = match.index ?? 0;
    out += stripMarks(line.slice(last, start)) + match[2].trim();
    last = start + match[0].length;
  }
  return out + stripMarks(line.slice(last));
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

/** 按码位截断，超出时加省略号；不会切开代理对（组合表情仍可能被截在中间） */
export function clipChars(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length > max ? `${chars.slice(0, max).join("").trimEnd()}…` : text;
}
