import { useMemo, type AnchorHTMLAttributes, type ComponentProps } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useT } from "@/i18n";
import { rehypeMark } from "./rehypeMark";

type RehypePlugins = NonNullable<ComponentProps<typeof ReactMarkdown>["rehypePlugins"]>;

const remarkPlugins = [remarkGfm];

/** 链接在系统浏览器中打开，避免 WebView 自身被导航走 */
function ExternalLink({
  href,
  children,
}: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const t = useT();
  const external = !!href && /^https?:\/\//i.test(href);
  return (
    <a
      href={href}
      title={external ? t("linkOpenExternal", { url: href ?? "" }) : undefined}
      onClick={(event) => {
        event.preventDefault();
        if (external) void openUrl(href).catch(() => {});
      }}
      className="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
    >
      {children}
    </a>
  );
}

const components: Components = { a: ExternalLink };

/**
 * 安全的 Markdown 渲染：不解析原始 HTML（react-markdown 默认跳过），
 * 代码块用 rehype-highlight 高亮，可选地把查找关键词包成 <mark>。
 */
export function Markdown({
  text,
  regex = null,
}: {
  text: string;
  regex?: RegExp | null;
}) {
  const rehypePlugins = useMemo<RehypePlugins>(() => {
    const list: RehypePlugins = [[rehypeHighlight, { detect: false }]];
    if (regex) list.push([rehypeMark, { regex }]);
    return list;
  }, [regex]);
  return (
    <div className="md-body">
      <ReactMarkdown
        remarkPlugins={remarkPlugins}
        rehypePlugins={rehypePlugins}
        components={components}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

/** 把代码包成 Markdown 围栏（围栏长度超过内容里最长的反引号串，内容里的 ``` 不会截断它） */
export function fencedCode(code: string, lang: string): string {
  const longest = Math.max(
    2,
    ...(code.match(/`+/g) ?? []).map((run) => run.length)
  );
  const fence = "`".repeat(longest + 1);
  return `${fence}${lang}\n${code}\n${fence}`;
}
