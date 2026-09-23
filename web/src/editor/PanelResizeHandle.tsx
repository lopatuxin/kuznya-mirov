import { useEffect, useRef, useState } from "react";
import { MAX_PANEL_WIDTH, MIN_PANEL_WIDTH } from "./useStoredPanelWidth";

type PanelResizeHandleProps = {
  /** Край панели, на котором стоит ручка: у левой панели — правый, у правой — левый. */
  edge: "left" | "right";
  width: number;
  onWidthChange: (width: number) => void;
  label: string;
};

const KEYBOARD_STEP = 16;
const RESIZING_BODY_CLASS = "is-resizing-panel";

/** Полоска на краю боковой панели: тянется мышью или стрелками с клавиатуры. */
export function PanelResizeHandle({ edge, width, onWidthChange, label }: PanelResizeHandleProps): React.JSX.Element {
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef<{ pointerX: number; width: number } | null>(null);
  const direction = edge === "right" ? 1 : -1;

  // Окно проекта закрыли посреди перетаскивания — курсор и запрет выделения текста не должны остаться на странице.
  useEffect(() => () => document.body.classList.remove(RESIZING_BODY_CLASS), []);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    // Узкое окно сжимает панель уже её сохранённой ширины — тянуть надо от той, что видна.
    const renderedWidth = event.currentTarget.parentElement?.getBoundingClientRect().width ?? width;
    dragStartRef.current = { pointerX: event.clientX, width: renderedWidth };
    setIsDragging(true);
    document.body.classList.add(RESIZING_BODY_CLASS);
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>): void {
    const dragStart = dragStartRef.current;
    if (dragStart === null) return;
    onWidthChange(dragStart.width + (event.clientX - dragStart.pointerX) * direction);
  }

  function handleDragEnd(): void {
    dragStartRef.current = null;
    setIsDragging(false);
    document.body.classList.remove(RESIZING_BODY_CLASS);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>): void {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const sign = event.key === "ArrowRight" ? 1 : -1;
    onWidthChange(width + KEYBOARD_STEP * sign * direction);
  }

  const className = [
    "panel-resize-handle",
    `panel-resize-handle--${edge}`,
    isDragging ? "panel-resize-handle--dragging" : "",
  ].join(" ");

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={MIN_PANEL_WIDTH}
      aria-valuemax={MAX_PANEL_WIDTH}
      tabIndex={0}
      className={className}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onLostPointerCapture={handleDragEnd}
      onKeyDown={handleKeyDown}
    />
  );
}
