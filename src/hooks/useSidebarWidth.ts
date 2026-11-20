// 侧栏宽度：拖动侧栏右边缘调整，记在本机。没调过时为 null，由布局按窗口宽度给默认值（264 / 248）。

import { useCallback, useState } from "react";

const STORAGE_KEY = "cchv-sidebar-width";
export const SIDEBAR_MIN_WIDTH = 220;
export const SIDEBAR_MAX_WIDTH = 520;
/** 主内容区至少保留的宽度；窗口变窄时侧栏随之收窄（见 sidebarWidthStyle） */
export const MAIN_MIN_WIDTH = 560;

export function clampSidebarWidth(width: number): number {
  return Math.round(
    Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, width))
  );
}

function readWidth(): number | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const value = raw === null ? Number.NaN : Number(raw);
    return Number.isFinite(value) ? clampSidebarWidth(value) : null;
  } catch {
    return null;
  }
}

function persist(width: number | null) {
  try {
    if (width === null) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, String(width));
  } catch {
    /* 忽略持久化失败 */
  }
}

export function useSidebarWidth() {
  const [width, setWidthState] = useState<number | null>(readWidth);

  /** 拖动过程中只更新界面，`done` 时才写入本机 */
  const resize = useCallback((next: number, done: boolean) => {
    const clamped = clampSidebarWidth(next);
    setWidthState(clamped);
    if (done) persist(clamped);
  }, []);

  const reset = useCallback(() => {
    setWidthState(null);
    persist(null);
  }, []);

  return { width, resize, reset };
}

/** 布局网格读取的 CSS 变量；窗口太窄时用 CSS clamp 让出主内容区 */
export function sidebarWidthValue(width: number): string {
  return `clamp(${SIDEBAR_MIN_WIDTH}px, ${width}px, calc(100vw - ${MAIN_MIN_WIDTH}px))`;
}
