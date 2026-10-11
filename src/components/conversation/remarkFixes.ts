// 对话 Markdown 的几处修正，都是作用在 mdast 上的 remark 插件：
// Codex 的建议追问指令、紧跟中文标点的裸网址、原始 HTML 里的换行与注释。

interface MdNode {
  type: string;
  value?: string;
  url?: string;
  children?: MdNode[];
  data?: { hName?: string; hProperties?: Record<string, unknown> };
}

/** 先序遍历所有带子节点的节点；回调可以替换 parent.children，随后遍历替换后的子节点 */
function eachParent(node: MdNode, visit: (parent: MdNode) => void) {
  if (!node.children) return;
  visit(node);
  for (const child of node.children) eachParent(child, visit);
}

/** Codex 回复末尾的 `:codex-followup[标签]{prompt="…"}` 之类的文本指令 */
const CODEX_DIRECTIVE = /:codex-[\w-]+\[([^\]\n]*)\]\{([^}\n]*)\}/g;

function splitDirectives(value: string): MdNode[] | null {
  // matchAll 会沿用正则的 lastIndex，先归零
  CODEX_DIRECTIVE.lastIndex = 0;
  const matches = [...value.matchAll(CODEX_DIRECTIVE)];
  if (matches.length === 0) return null;
  const nodes: MdNode[] = [];
  let last = 0;
  for (const match of matches) {
    const start = match.index ?? 0;
    if (start > last) nodes.push({ type: "text", value: value.slice(last, start) });
    const label = match[1].trim();
    const prompt = /prompt="([^"]*)"/.exec(match[2])?.[1] ?? "";
    nodes.push({
      type: "codexDirective",
      data: {
        hName: "span",
        hProperties: { className: ["codex-directive"], title: prompt || undefined },
      },
      children: [{ type: "text", value: label || prompt }],
    });
    last = start + match[0].length;
  }
  if (last < value.length) nodes.push({ type: "text", value: value.slice(last) });
  return nodes;
}

/** 把 Codex 指令渲染成「↪ 标签」小标签（样式见 .codex-directive），悬停看完整提问 */
export function remarkCodexDirectives() {
  return (tree: unknown) =>
    eachParent(tree as MdNode, (parent) => {
      if (!parent.children!.some((child) => child.type === "text")) return;
      parent.children = parent.children!.flatMap((child) =>
        child.type === "text" && child.value
          ? (splitDirectives(child.value) ?? [child])
          : [child]
      );
    });
}

/** 网址里不会出现未编码的中文标点与全角符号：裸网址遇到它们即结束 */
const CJK_PUNCTUATION = /[—‘-‟…　-〿！-／：-＠［-｀｛-･]/;

/**
 * GFM 的裸网址只在空白与 ASCII 标点处结束，「详见 https://a.com/x，然后…」会把后面的中文
 * 一起吞进链接。只处理链接文字就是网址本身的情况（裸网址与 `<…>` 自动链接），把中文标点
 * 及其后的内容还给正文。
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
        if (!link.url.endsWith(label.value)) continue;
        const cut = label.value.search(CJK_PUNCTUATION);
        if (cut <= 0) continue;
        const rest = label.value.slice(cut);
        label.value = label.value.slice(0, cut);
        link.url = link.url.slice(0, link.url.length - rest.length);
        children.splice(index + 1, 0, { type: "text", value: rest });
      }
    });
}

const BREAK_TAG = /^<br\s*\/?>$/i;
const HTML_COMMENT = /^<!--[\s\S]*-->$/;

/**
 * 原始 HTML 仍按文本显示（不解析、不执行），只做两件安全的事：
 * `<br>` 换成换行（表格单元格里常见），HTML 注释直接去掉。
 */
export function remarkLiteHtml() {
  return (tree: unknown) =>
    eachParent(tree as MdNode, (parent) => {
      if (!parent.children!.some((child) => child.type === "html")) return;
      parent.children = parent.children!.flatMap((child): MdNode[] => {
        if (child.type !== "html" || child.value === undefined) return [child];
        const value = child.value.trim();
        if (BREAK_TAG.test(value)) return [{ type: "break" }];
        if (HTML_COMMENT.test(value)) return [];
        return [child];
      });
    });
}
