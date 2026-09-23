import type { Engine } from "engine";
import { useEffect, useRef, type RefObject } from "react";
import { computeCanvasLayout } from "../canvasLayout";
import type { SceneSize } from "./sceneObjects";
import { fitSceneStage } from "./sceneStageLayout";

type SceneCanvasProps = {
  /** Тот же холст, на котором `useProjectEngine` создал движок — требование Engine.create(canvas). */
  canvasRef: RefObject<HTMLCanvasElement | null>;
  engine: Engine | null;
  sceneSize: SceneSize | null;
  selectedIndex: number | null;
  /** Подпись над рамкой выбранного объекта — его имя или номер. */
  selectedLabel: string | null;
  onSelect: (index: number | null) => void;
};

type CanvasRect = { x: number; y: number; width: number; height: number };

const STAGE_PADDING = 28;
const SELECTION_COLOR = "#5ab0ff";
const SELECTION_FILL = "rgba(90, 176, 255, 0.1)";
const SELECTION_HALO = "rgba(6, 8, 14, 0.6)";
const LABEL_TEXT_COLOR = "#06121f";
const LABEL_FONT = '600 11px "EditorUi", "Segoe UI", sans-serif';
const LABEL_HEIGHT = 18;
const LABEL_GAP = 4;

function placeSelectionLabel(rect: CanvasRect, labelWidth: number, canvasWidth: number, canvasHeight: number): { x: number; y: number } {
  const x = Math.min(Math.max(0, rect.x), Math.max(0, canvasWidth - labelWidth));
  const above = rect.y - LABEL_HEIGHT - LABEL_GAP;
  if (above >= 0) return { x, y: above };
  const below = rect.y + rect.height + LABEL_GAP;
  if (below + LABEL_HEIGHT <= canvasHeight) return { x, y: below };
  return { x, y: Math.max(0, rect.y) + LABEL_GAP };
}

function drawSelection(context: CanvasRenderingContext2D, rect: CanvasRect, label: string | null, pixelRatio: number): void {
  const canvasWidth = context.canvas.width / pixelRatio;
  const canvasHeight = context.canvas.height / pixelRatio;
  const frameX = rect.x + 1;
  const frameY = rect.y + 1;
  const frameWidth = Math.max(0, rect.width - 2);
  const frameHeight = Math.max(0, rect.height - 2);

  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.fillStyle = SELECTION_FILL;
  context.fillRect(rect.x, rect.y, rect.width, rect.height);
  // Тёмная кайма под рамкой — чтобы рамку было видно и на голубом, и на светлом объекте.
  context.lineWidth = 4;
  context.strokeStyle = SELECTION_HALO;
  context.strokeRect(frameX, frameY, frameWidth, frameHeight);
  context.lineWidth = 2;
  context.strokeStyle = SELECTION_COLOR;
  context.strokeRect(frameX, frameY, frameWidth, frameHeight);

  if (label !== null) {
    context.font = LABEL_FONT;
    const labelWidth = Math.ceil(context.measureText(label).width) + 12;
    const position = placeSelectionLabel(rect, labelWidth, canvasWidth, canvasHeight);
    context.fillStyle = SELECTION_COLOR;
    context.beginPath();
    context.roundRect(position.x, position.y, labelWidth, LABEL_HEIGHT, 4);
    context.fill();
    context.fillStyle = LABEL_TEXT_COLOR;
    context.textBaseline = "middle";
    context.fillText(label, position.x + 6, position.y + LABEL_HEIGHT / 2 + 0.5);
  }
  context.restore();
}

/**
 * Сцена вписана в свою часть окна с сохранением пропорций («Редактор», требование 22) на одном
 * холсте, а рамку выбранного объекта поверх него рисует сам редактор на втором, прозрачном
 * («Решения» — «рамку выбора рисует редактор, а не движок», требование 24). Щелчок по холсту —
 * `object_at` (требование 23), щелчок по полям вокруг сцены снимает выбор. За размером следит
 * `ResizeObserver` на части окна со сценой, а не `window.resize`, как у страницы игры.
 */
export function SceneCanvas({ canvasRef, engine, sceneSize, selectedIndex, selectedLabel, onSelect }: SceneCanvasProps): React.JSX.Element {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const selectedIndexRef = useRef(selectedIndex);
  selectedIndexRef.current = selectedIndex;
  const selectedLabelRef = useRef(selectedLabel);
  selectedLabelRef.current = selectedLabel;
  const sceneWidth = sceneSize?.width ?? null;
  const sceneHeight = sceneSize?.height ?? null;

  useEffect(() => {
    const area = areaRef.current;
    const stage = stageRef.current;
    const sceneCanvas = canvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!engine || !area || !stage || !sceneCanvas || !overlayCanvas) return;
    const activeEngine = engine;
    const knownSceneSize = sceneWidth !== null && sceneHeight !== null ? { width: sceneWidth, height: sceneHeight } : null;

    function applyLayout(areaWidth: number, areaHeight: number): void {
      const stageSize = fitSceneStage(areaWidth, areaHeight, knownSceneSize, STAGE_PADDING);
      const pixelRatio = window.devicePixelRatio || 1;
      const layout = computeCanvasLayout(stageSize.width, stageSize.height, pixelRatio);
      if (!stage) return;
      stage.style.width = `${layout.cssWidth}px`;
      stage.style.height = `${layout.cssHeight}px`;
      for (const canvas of [sceneCanvas, overlayCanvas]) {
        if (!canvas) continue;
        canvas.style.width = `${layout.cssWidth}px`;
        canvas.style.height = `${layout.cssHeight}px`;
        canvas.width = layout.bufferWidth;
        canvas.height = layout.bufferHeight;
      }
      activeEngine.resize(layout.bufferWidth, layout.bufferHeight);
      activeEngine.set_pixel_ratio(pixelRatio);
    }

    applyLayout(area.clientWidth, area.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) applyLayout(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(area);

    return () => observer.disconnect();
  }, [canvasRef, engine, sceneWidth, sceneHeight]);

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
          const rect = activeEngine.object_rect(selected) as CanvasRect | undefined;
          if (rect) drawSelection(overlayContext, rect, selectedLabelRef.current, window.devicePixelRatio || 1);
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

  function handleCanvasClick(event: React.MouseEvent<HTMLCanvasElement>): void {
    if (!engine) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const result = engine.object_at(event.clientX - bounds.left, event.clientY - bounds.top) as number | undefined;
    onSelect(typeof result === "number" ? result : null);
  }

  function handleAreaClick(event: React.MouseEvent<HTMLDivElement>): void {
    if (event.target === event.currentTarget) onSelect(null);
  }

  return (
    <div ref={areaRef} className="scene-view" onClick={handleAreaClick}>
      <div ref={stageRef} className="scene-view__stage">
        <canvas ref={canvasRef} className="scene-view__world" />
        <canvas ref={overlayCanvasRef} className="scene-view__overlay" onClick={handleCanvasClick} />
      </div>
      {sceneSize !== null && (
        <span className="scene-view__size">
          Сцена {sceneSize.width} × {sceneSize.height}
        </span>
      )}
    </div>
  );
}
