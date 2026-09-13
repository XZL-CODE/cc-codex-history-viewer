// 会话内查找共用的文本匹配工具：把多个关键词编译成一个大小写不敏感的正则，
// 并把文本切成「命中 / 未命中」片段供 React 与 rehype 两条渲染路径共用。

export function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** 空格分隔的关键词 → 捕获组正则；没有有效关键词时返回 null */
export function buildTokenRegex(tokens: string[]): RegExp | null {
  const clean = Array.from(
    new Set(tokens.map((token) => token.trim()).filter(Boolean))
  );
  if (clean.length === 0) return null;
  // 长词优先，避免短词抢先匹配导致长词被拆开
  clean.sort((a, b) => b.length - a.length);
  return new RegExp(`(${clean.map(escapeRegExp).join("|")})`, "giu");
}

export interface TextPart {
  text: string;
  hit: boolean;
}

/** 用捕获组正则切分文本：奇数下标即命中片段 */
export function splitByRegex(text: string, regex: RegExp): TextPart[] {
  return text
    .split(regex)
    .map((part, index) => ({ text: part, hit: index % 2 === 1 }))
    .filter((part) => part.text.length > 0);
}

export function hasMatch(text: string, regex: RegExp): boolean {
  regex.lastIndex = 0;
  const matched = regex.test(text);
  regex.lastIndex = 0;
  return matched;
}
