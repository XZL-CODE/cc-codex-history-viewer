// 更新弹窗里的更新说明：去掉 Release 正文里弹窗用不上的部分，再按界面语言只留中文或英文。
// Release 正文是中英双语，先列中文条目、再列英文条目（见 .github/release-notes.md），没有逐条标注语言，
// 所以逐项按文字判断：汉字多的算中文，只有英文单词的算英文，两样都没有的（如分隔线）两种语言都保留。

import type { LangCode } from "@/i18n";

/** Release 正文末尾是给手动下载的人看的安装说明，应用内更新用不上 */
const INSTALL_NOTES_HEADING = /^##\s+(安装说明|Install notes)/m;
/** 正文开头的「## v0.14.0 更新内容 / What's new」，弹窗另有版本标题 */
const VERSION_HEADING = /^#{1,3}\s+v?\d+\.\d+\.\d+\b.*\n*/;

/** 顶格的列表项或标题开始新的一项；缩进两格及以上的行属于上一项（嵌套列表、续行） */
const ITEM_START = /^ ?(?:[-*+]\s|\d+[.)]\s|#{1,6}\s)/;
const INDENTED = /^(?: {2,}|\t)/;
const FENCE = /^ {0,3}(?:```|~~~)/;
const FENCED_BLOCK = /^ *(```|~~~)[^\n]*\n[\s\S]*?^ *\1[^\n]*$/gm;
const HEADING = /^(#{1,6})\s+(.*)$/;

const HAN = /\p{Script=Han}/gu;
const LATIN_WORD = /[A-Za-z]+/g;
/** 中文条目常夹着产品名等英文词，英文条目偶尔引用一两个中文词：汉字数的 4 倍不少于英文单词数就算中文 */
const HAN_WEIGHT = 4;

/** 去掉安装说明与开头的版本标题，统一换行符 */
export function cleanReleaseNotes(notes: string | null): string {
  if (!notes) return "";
  const text = notes.replace(/\r\n?/g, "\n");
  const cut = text.search(INSTALL_NOTES_HEADING);
  const body = cut >= 0 ? text.slice(0, cut) : text;
  return body.trim().replace(VERSION_HEADING, "").trim();
}

interface Item {
  text: string;
  /** 原文里这一项前面有空行：拼回去时保留，列表的紧凑与否不变 */
  blankBefore: boolean;
}

function splitItems(text: string): Item[] {
  const items: { lines: string[]; blankBefore: boolean }[] = [];
  let current: { lines: string[]; blankBefore: boolean } | null = null;
  let blank = false;
  let inFence = false;
  for (const line of text.split("\n")) {
    if (inFence || !line.trim()) {
      current?.lines.push(line);
      if (FENCE.test(line)) inFence = !inFence;
      else if (!inFence) blank = true;
      continue;
    }
    const startsItem =
      !INDENTED.test(line) && (blank || ITEM_START.test(line));
    if (!current || startsItem) {
      current = { lines: [line], blankBefore: blank };
      items.push(current);
    } else {
      current.lines.push(line);
    }
    if (FENCE.test(line)) inFence = true;
    blank = false;
  }
  return items.map((item) => ({
    text: item.lines.join("\n").trim(),
    blankBefore: item.blankBefore,
  }));
}

/** 一项的语言；没有文字时为 null */
function textLang(text: string): LangCode | null {
  // 代码与链接地址不算：中文条目里也常写 `~/.gemini`、`xhigh`；单独的代码块两种语言都保留
  const prose = text
    .replace(FENCED_BLOCK, " ")
    .replace(/`[^`\n]*`/g, " ")
    .replace(/\]\([^)]*\)/g, "]");
  const han = prose.match(HAN)?.length ?? 0;
  const words = prose.match(LATIN_WORD)?.length ?? 0;
  if (han === 0) return words > 0 ? "en" : null;
  return han * HAN_WEIGHT >= words ? "zh" : "en";
}

/** 「### 修复 / Fixes」这类双语标题只留界面语言那一半 */
function headingForLang(text: string, lang: LangCode): string | null {
  const match = HEADING.exec(text);
  if (!match || text.includes("\n")) return null;
  const part = match[2]
    .split(" / ")
    .find((half) => textLang(half) === lang);
  return part ? `${match[1]} ${part.trim()}` : null;
}

/**
 * 只留界面语言的条目。分不出另一种语言（整段只有一种语言或判断不出）时原样返回，
 * 宁可多显示也不让说明变空。
 */
export function notesForLang(notes: string, lang: LangCode): string {
  const items = splitItems(notes);
  const kept: Item[] = [];
  let other = false;
  for (const item of items) {
    const heading = headingForLang(item.text, lang);
    if (heading) {
      kept.push({ ...item, text: heading });
      continue;
    }
    const itemLang = textLang(item.text);
    if (itemLang === null || itemLang === lang) kept.push(item);
    else other = true;
  }
  if (!other || !kept.some((item) => textLang(item.text) === lang)) {
    return notes;
  }
  return kept
    .map((item, index) =>
      index === 0 ? item.text : (item.blankBefore ? "\n\n" : "\n") + item.text
    )
    .join("");
}
