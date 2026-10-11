import {
  createContext,
  useContext,
  useMemo,
  type AnchorHTMLAttributes,
  type ComponentProps,
  type ImgHTMLAttributes,
} from "react";
import ReactMarkdown, {
  defaultUrlTransform,
  type Components,
  type UrlTransform,
} from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import remarkCjkFriendly from "remark-cjk-friendly/parseOnly";
import remarkCjkFriendlyStrikethrough from "remark-cjk-friendly-gfm-strikethrough/parseOnly";
import rehypeHighlight from "rehype-highlight";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Image as ImageIcon } from "lucide-react";
import { useT } from "@/i18n";
import { rehypeMark } from "./rehypeMark";
import {
  codexDirectivesToLinks,
  DIRECTIVE_SCHEME,
  remarkLiteHtml,
  remarkTrimCjkAutolinks,
} from "./remarkFixes";

type RehypePlugins = NonNullable<ComponentProps<typeof ReactMarkdown>["rehypePlugins"]>;

// - CommonMark 的强调与删除线规则在中文标点旁失效（如「**不会当作零。**其他」不加粗），
//   remark-cjk-friendly 两个插件按 CJK 友好的修订规则补上，只用于解析
// - 段内单个换行按换行显示（remark-breaks），与 GitHub 评论和「原文」模式一致
// - remarkFixes：裸网址后的中文标点、<br> 与 HTML 注释；Codex 指令在解析前改写成链接
const remarkPlugins = [
  remarkGfm,
  remarkCjkFriendly,
  remarkCjkFriendlyStrikethrough,
  remarkBreaks,
  remarkTrimCjkAutolinks,
  remarkLiteHtml,
];

/** Windows 盘符路径（`/` 或 `\`）与 file:// 地址：默认的地址过滤会把它们清空 */
const LOCAL_TARGET = /^([A-Za-z]:[\\/]|file:\/\/)/i;

/** 链接目标在渲染前已被百分号编码；只用于显示，解不开时原样返回 */
function decodeTarget(url: string): string {
  try {
    return decodeURI(url);
  } catch {
    return url;
  }
}

/**
 * 链接目标保留本地路径与 Codex 指令（都只用于显示，见 ExternalLink），
 * 其余按 react-markdown 的默认规则过滤（javascript: 等不安全协议被清空）。
 */
const urlTransform: UrlTransform = (url, key) => {
  if (key === "href") {
    if (url.startsWith(DIRECTIVE_SCHEME)) return url;
    if (LOCAL_TARGET.test(decodeTarget(url))) return url;
  }
  return defaultUrlTransform(url);
};

/**
 * 链接在系统浏览器中打开，避免 WebView 自身被导航走。
 * 不是 http(s) 的目标（本地文件、相对路径、锚点）不可点击，悬停显示目标：外部会话里的
 * 路径属于别人的电脑，本机会话里也不替用户打开文件。
 */
function ExternalLink({
  href,
  children,
}: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const t = useT();
  const external = !!href && /^https?:\/\//i.test(href);
  if (href?.startsWith(DIRECTIVE_SCHEME)) {
    // Codex 的建议追问：「↪ 标签」，悬停看完整提问
    let detail = href.slice(DIRECTIVE_SCHEME.length);
    try {
      detail = decodeURIComponent(detail);
    } catch {
      /* 保留编码后的原文 */
    }
    return (
      <span className="codex-directive" title={detail}>
        {children}
      </span>
    );
  }
  if (!external) {
    return (
      <span
        className="md-local-link"
        title={href ? t("linkLocalTarget", { target: decodeTarget(href) }) : undefined}
      >
        {children}
      </span>
    );
  }
  return (
    <a
      href={href}
      title={t("linkOpenExternal", { url: href })}
      onClick={(event) => {
        event.preventDefault();
        void openUrl(href).catch(() => {});
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
 * 安全的 Markdown 渲染：原始 HTML 不解析、按文本显示（只把 <br> 换成换行、去掉注释），
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
  const source = useMemo(() => codexDirectivesToLinks(text), [text]);
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
        urlTransform={urlTransform}
      >
        {source}
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
