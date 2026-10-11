import {
  createContext,
  useContext,
  useMemo,
  type AnchorHTMLAttributes,
  type ComponentProps,
  type ImgHTMLAttributes,
} from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkCjkFriendly from "remark-cjk-friendly/parseOnly";
import rehypeHighlight from "rehype-highlight";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Image as ImageIcon } from "lucide-react";
import { useT } from "@/i18n";
import { rehypeMark } from "./rehypeMark";

type RehypePlugins = NonNullable<ComponentProps<typeof ReactMarkdown>["rehypePlugins"]>;

// CommonMark 的强调规则在中文标点旁失效（如「**不会当作零。**其他」不加粗），
// remark-cjk-friendly 按 CJK 友好的修订规则补上；只用于解析
const remarkPlugins = [remarkGfm, remarkCjkFriendly];

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

/**
 * 为 true 时远程图片不加载，只显示为链接，点击才交给系统浏览器。
 * 外部会话来自他人，打开即加载远程图片会把阅读行为暴露给图片所在的服务器。
 */
export const RemoteImagesBlockedContext = createContext(false);

/**
 * 按浏览器加载图片时的同一套规则解析地址：落到别的 http(s) 源即为远程图片
 * （含 `//host`、`\\host`、`https:host` 这类写法），返回可在浏览器打开的地址。
 */
function remoteImageUrl(src: string | undefined): string | null {
  if (!src) return null;
  try {
    const url = new URL(src, window.location.href);
    const web = url.protocol === "http:" || url.protocol === "https:";
    return web && url.origin !== window.location.origin ? url.href : null;
  } catch {
    return null;
  }
}

/** 远程图片换成链接；本地与内联（data:）图片照常显示 */
function LinkedImage({ src, alt }: ImgHTMLAttributes<HTMLImageElement>) {
  const t = useT();
  const url = remoteImageUrl(src);
  if (!url) return <img src={src} alt={alt} />;
  return (
    <ExternalLink href={url}>
      <ImageIcon size={12} className="mr-1 inline-block align-[-1px]" aria-hidden />
      {t("remoteImageLink", { label: alt || url })}
    </ExternalLink>
  );
}

const components: Components = { a: ExternalLink };
const remoteImagesBlocked: Components = { a: ExternalLink, img: LinkedImage };

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
  const blockRemoteImages = useContext(RemoteImagesBlockedContext);
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
        components={blockRemoteImages ? remoteImagesBlocked : components}
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
