// 侧栏右边缘的拖动条：拖动调整宽度，双击恢复默认；聚焦后可用左右方向键微调。

import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { useT } from "@/i18n";

const KEYBOARD_STEP = 16;

export function SidebarResizer({
  onResize,
  onReset,
}: {
  /** 新宽度（像素）；`done` 为 true 时是拖动结束或键盘调整 */
  onResize: (width: number, done: boolean) => void;
  onReset: () => void;
}) {
  const t = useT();
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);

  // 拖动条放在侧栏容器里，起始宽度取容器当前的实际宽度（没调过时是默认宽度）
  const currentWidth = (element: HTMLElement) =>
    element.parentElement?.getBoundingClientRect().width ?? 0;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      startX: event.clientX,
      startWidth: currentWidth(event.currentTarget),
    };
    document.body.classList.add("sidebar-resizing");
  };

  const widthAt = (event: PointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    return state ? state.startWidth + event.clientX - state.startX : null;
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const width = widthAt(event);
    if (width !== null) onResize(width, false);
  };

  const finish = (event: PointerEvent<HTMLDivElement>) => {
    const width = widthAt(event);
    drag.current = null;
    document.body.classList.remove("sidebar-resizing");
    if (width !== null) onResize(width, true);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step =
      event.key === "ArrowLeft"
        ? -KEYBOARD_STEP
        : event.key === "ArrowRight"
          ? KEYBOARD_STEP
          : 0;
    if (!step) return;
    event.preventDefault();
    onResize(currentWidth(event.currentTarget) + step, true);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t("sidebarResizeLabel")}
      title={t("sidebarResizeTitle")}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={finish}
      onPointerCancel={finish}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
      className="group absolute inset-y-0 -right-1 z-20 w-2 cursor-col-resize touch-none outline-none"
    >
      <span
        aria-hidden
        className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-accent opacity-0 transition-opacity group-hover:opacity-60 group-focus-visible:opacity-100 group-active:opacity-100"
      />
    </div>
  );
}
