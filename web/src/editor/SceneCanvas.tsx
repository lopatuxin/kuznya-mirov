import type { Engine } from "engine";
import { useEffect, useRef, type RefObject } from "react";
import { computeCanvasLayout } from "../canvasLayout";

type SceneCanvasProps = {
  /** Тот же холст, на котором `useProjectEngine` создал движок — требование Engine.create(canvas). */
  canvasRef: RefObject<HTMLCanvasElement | null>;
  engine: Engine | null;
  selectedIndex: number | null;
  onSelect: (index: number | null) => void;
};

const SELECTION_COLOR = "#5ab0ff";
const SELECTION_LINE_WIDTH = 2;

function drawSelectionFrame(
  context: CanvasRenderingContext2D,
  rect: { x: number; y: number; width: number; height: number },
  pixelRatio: number,
): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.strokeStyle = SELECTION_COLOR;
  context.lineWidth = SELECTION_LINE_WIDTH;
  const inset = SELECTION_LINE_WIDTH / 2;
  context.strokeRect(
    rect.x + inset,
    rect.y + inset,
    Math.max(0, rect.width - SELECTION_LINE_WIDTH),
    Math.max(0, rect.height - SELECTION_LINE_WIDTH),
  );
  context.restore();
}

/**
 * Сцена вписана в свою часть окна («Редактор», требование 22) на одном холсте, а рамку выбранного
 * объекта поверх него рисует сам редактор на втором, прозрачном («Решения» —
 * «рамку выбора рисует редактор, а не движок», требование 24). Клик по любому из них — `object_at`
 * (требование 23); за размером следит `ResizeObserver` на общем контейнере, а не `window.resize`,
 * как у страницы игры — холст здесь меньше окна.
 */
export function SceneCanvas({ canvasRef, engine, selectedIndex, onSelect }: SceneCanvasProps): React.JSX.Element {
  const containerRef = useRef<HTMLDivElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const selectedIndexRef = useRef(selectedIndex);
  selectedIndexRef.current = selectedIndex;

  useEffect(() => {
    const container = containerRef.current;
    const sceneCanvas = canvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!engine || !container || !sceneCanvas || !overlayCanvas) return;
    const activeEngine = engine;

    function applyLayout(width: number, height: number): void {
      const layout = computeCanvasLayout(width, height, window.devicePixelRatio || 1);
      for (const canvas of [sceneCanvas, overlayCanvas]) {
        if (!canvas) continue;
        canvas.style.width = `${layout.cssWidth}px`;
        canvas.style.height = `${layout.cssHeight}px`;
        canvas.width = layout.bufferWidth;
        canvas.height = layout.bufferHeight;
      }
      activeEngine.resize(layout.bufferWidth, layout.bufferHeight);
      activeEngine.set_pixel_ratio(window.devicePixelRatio || 1);
    }

    applyLayout(container.clientWidth, container.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) applyLayout(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(container);

    return () => observer.disconnect();
  }, [canvasRef, engine]);

  useEffect(() => {
    if (!engine) return;
    const activeEngine = engine;
    const overlayCanvas = overlayCanvasRef.current;
    const overlayContext = overlayCanvas?.getContext("2d") ?? null;
    let frameHandle = 0;
    let stopped = false;

    function frame(): void {
      if (stopped) return;
      activeEngine.draw();
      if (overlayCanvas && overlayContext) {
        overlayContext.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        const selected = selectedIndexRef.current;
        // Нефункциональное требование: пока ничего не выбрано, object_rect не зовётся.
        if (selected !== null) {
          const rect = activeEngine.object_rect(selected) as { x: number; y: number; width: number; height: number } | undefined;
          if (rect) drawSelectionFrame(overlayContext, rect, window.devicePixelRatio || 1);
        }
      }
      frameHandle = requestAnimationFrame(frame);
    }
    frameHandle = requestAnimationFrame(frame);

    return () => {
      stopped = true;
      cancelAnimationFrame(frameHandle);
    };
  }, [engine]);

  function handleClick(event: React.MouseEvent<HTMLCanvasElement>): void {
    if (!engine) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const result = engine.object_at(event.clientX - bounds.left, event.clientY - bounds.top) as number | undefined;
    onSelect(typeof result === "number" ? result : null);
  }

  return (
    <div ref={containerRef} className="scene-canvas">
      <canvas ref={canvasRef} className="scene-canvas__world" />
      <canvas ref={overlayCanvasRef} className="scene-canvas__overlay" onClick={handleClick} />
    </div>
  );
}
