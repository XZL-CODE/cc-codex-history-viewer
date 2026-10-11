// 对话 Markdown 的几处修正：Codex 的文本指令（解析前的源码预处理）、紧跟中文标点的裸网址
// 与原始 HTML 里的换行和注释（作用在 mdast 上的 remark 插件）。

interface MdNode {
  type: string;
  value?: string;
  url?: string;
  children?: MdNode[];
}

/** 先序遍历所有带子节点的节点；回调可以替换 parent.children，随后遍历替换后的子节点 */
function eachParent(node: MdNode, visit: (parent: MdNode) => void) {
  if (!node.children) return;
  visit(node);
  for (const child of node.children) eachParent(child, visit);
}

// ----------------------------- Codex 文本指令 -----------------------------

/** 指令改写成链接后用的私有协议；Markdown 组件据此渲染成「↪ 标签」小标签 */
export const DIRECTIVE_SCHEME = "codex-directive:";

/**
 * Codex 回复里的文本指令：`:codex-followup[标签]{prompt="…"}`、`:codex-file-citation{path="…"}`。
 * 属性值可以带转义的引号；前一个字符是反引号（行内代码）或字母数字时不算。
 */
const DIRECTIVE =
  /(^|[^`\w]):codex-[\w-]+(?:\[([^\]\n]*)\])?\{((?:[^{}"\n]|"(?:[^"\\\n]|\\.)*")*)\}/g;
const ATTRIBUTE = /([\w-]+)="((?:[^"\\]|\\.)*)"/g;
const FENCE_MARKER = /^\s{0,3}(`{3,}|~{3,})/;

function directiveLink(label: string | undefined, attrs: string): string | null {
  const values: Record<string, string> = {};
  for (const match of attrs.matchAll(ATTRIBUTE)) {
    values[match[1]] = match[2].replace(/\\(.)/g, "$1");
  }
  const path = values.path ? values.path.split(/[\\/]/).pop() : undefined;
  const text = label?.trim() || path || values.prompt || "";
  if (!text) return null;
  const detail = values.prompt ?? values.path ?? text;
  // 括号会提前结束链接目标，额外编码
  const href =
    DIRECTIVE_SCHEME +
    encodeURIComponent(detail).replace(/[()]/g, (c) => (c === "(" ? "%28" : "%29"));
  const escaped = text.replace(/[\\`*_[\]<>~|!#]/g, "\\$&");
  return `[${escaped}](${href})`;
}

/**
 * 把 Codex 指令改写成 `[标签](codex-directive:…)` 链接再交给解析器：指令里的反引号、网址
 * 不会把它拆成几段。围栏代码块里的内容不动。
 */
export function codexDirectivesToLinks(markdown: string): string {
  if (!markdown.includes(":codex-")) return markdown;
  let fence: string | null = null;
  return markdown
    .split("\n")
    .map((line) => {
      const marker = FENCE_MARKER.exec(line)?.[1];
      if (fence) {
        const closes =
          marker !== undefined &&
          marker[0] === fence[0] &&
          marker.length >= fence.length &&
          line.trim() === marker;
        if (closes) fence = null;
        return line;
      }
      if (marker) {
        fence = marker;
        return line;
      }
      return line.replace(DIRECTIVE, (all, before: string, label?: string, attrs = "") => {
        const link = directiveLink(label, attrs);
        return link ? before + link : all;
      });
    })
    .join("\n");
}

// ----------------------------- 裸网址 -----------------------------

/** 网址里不会出现未编码的中文标点与全角符号：裸网址遇到它们即结束 */
const CJK_PUNCTUATION = /[—‘-‟…　-〿！-／：-＠［-｀｛-･]/;
/** 被截下来的剩余文字里可能还有网址（如「https://a.com/x、https://b.com/y」） */
const BARE_URL = /https?:\/\/[^\s<]+/;
const TRAILING_PUNCTUATION = /[.,:;!?'")\]]+$/;

/** 链接文字就是网址本身：裸网址、www. 与 `<…>` 自动链接；手写的 [文字](目标) 不算 */
function isAutolink(url: string, label: string): boolean {
  return url === label || url === `http://${label}` || url === `mailto:${label}`;
}

/** 把一段文字里的 http(s) 网址做成链接，网址在中文标点或末尾的 ASCII 标点处结束 */
function linkify(text: string): MdNode[] {
  const nodes: MdNode[] = [];
  const pattern = new RegExp(BARE_URL.source, "g");
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    const start = match.index;
    let url = match[0];
    const cut = url.search(CJK_PUNCTUATION);
    if (cut >= 0) url = url.slice(0, cut);
    url = url.replace(TRAILING_PUNCTUATION, "");
    if (!/^https?:\/\/./.test(url)) {
      pattern.lastIndex = start + 1;
      continue;
    }
    if (start > last) nodes.push({ type: "text", value: text.slice(last, start) });
    nodes.push({ type: "link", url, children: [{ type: "text", value: url }] });
    // 从网址结束处继续找：「https://b.com/y、https://c.com/z」里的第二个网址也要识别
    last = start + url.length;
    pattern.lastIndex = last;
  }
  if (last < text.length) nodes.push({ type: "text", value: text.slice(last) });
  return nodes;
}

/**
 * GFM 的裸网址只在空白与 ASCII 标点处结束，「详见 https://a.com/x，然后…」会把后面的中文
 * 一起吞进链接。只处理自动链接，把中文标点及其后的内容还给正文，剩余文字里的网址重新识别。
 */
export function remarkTrimCjkAutolinks() {
  return (tree: unknown) =>
    eachParent(tree as MdNode, (parent) => {
      const children = parent.children!;
      for (let index = 0; index < children.length; index += 1) {
        const link = children[index];
        const label = link.children?.length === 1 ? link.children[0] : null;
        if (link.type !== "link" || label?.type !== "text" || !label.value || !link.url) {
          continue;
        }
        if (!isAutolink(link.url, label.value)) continue;
        const cut = label.value.search(CJK_PUNCTUATION);
        if (cut <= 0) continue;
        const rest = label.value.slice(cut);
        label.value = label.value.slice(0, cut);
        link.url = link.url.slice(0, link.url.length - rest.length);
        const tail = linkify(rest);
        children.splice(index + 1, 0, ...tail);
        index += tail.length;
      }
    });
}

// ----------------------------- 原始 HTML -----------------------------

const BREAK_TAG = /^<br\s*\/?>$/i;
/** 只匹配单个注释；「<!-- a --> 正文 <!-- b -->」整行不是注释 */
const HTML_COMMENT = /^<!--(?:(?!-->)[\s\S])*-->$/;

/**
 * 原始 HTML 仍按文本显示（不解析、不执行），只做两件安全的事：`<br>` 换成换行（表格单元格
 * 里常见；后面紧跟的换行已被 remark-breaks 变成换行时不再重复），HTML 注释直接去掉。
 */
export function remarkLiteHtml() {
  return (tree: unknown) =>
    eachParent(tree as MdNode, (parent) => {
      const children = parent.children!;
      if (!children.some((child) => child.type === "html")) return;
      parent.children = children.flatMap((child, index): MdNode[] => {
        if (child.type !== "html" || child.value === undefined) return [child];
        const value = child.value.trim();
        if (BREAK_TAG.test(value)) {
          return children[index + 1]?.type === "break" ? [] : [{ type: "break" }];
        }
        if (HTML_COMMENT.test(value)) return [];
        return [child];
      });
    });
}
