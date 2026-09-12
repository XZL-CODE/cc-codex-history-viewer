// rehype 插件：把 Markdown 渲染树里命中查找关键词的文本包进 <mark class="find-hit">。
// 只做最小的 hast 遍历，不引入额外依赖。

import { splitByRegex } from "@/lib/textMatch";

interface HastNode {
  type: string;
  value?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
}

function markNode(text: string): HastNode {
  return {
    type: "element",
    tagName: "mark",
    properties: { className: ["find-hit"] },
    children: [{ type: "text", value: text }],
  };
}

function walk(node: HastNode, regex: RegExp) {
  if (!node.children) return;
  const next: HastNode[] = [];
  for (const child of node.children) {
    if (child.type === "text" && child.value) {
      const parts = splitByRegex(child.value, regex);
      if (parts.length === 1 && !parts[0].hit) {
        next.push(child);
        continue;
      }
      for (const part of parts) {
        next.push(part.hit ? markNode(part.text) : { type: "text", value: part.text });
      }
      continue;
    }
    if (child.type === "element" && child.tagName !== "mark") {
      walk(child, regex);
    }
    next.push(child);
  }
  node.children = next;
}

export function rehypeMark(options: { regex: RegExp | null }) {
  return (tree: HastNode) => {
    if (options.regex) walk(tree, options.regex);
  };
}
