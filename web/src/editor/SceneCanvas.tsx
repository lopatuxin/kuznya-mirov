import type { Engine } from "engine";
import { useEffect, useRef, type RefObject } from "react";
import { computeCanvasLayout } from "../canvasLayout";
import { cellSizeFromObjectRect, computeDragPosition, hasCrossedDragThreshold } from "./dragPlacement";
import type { SceneSize } from "./sceneObjects";
import { fitSceneStage } from "./sceneStageLayout";

type ObjectGeometry = { position: readonly [number, number]; size: readonly [number, number] };

type SceneCanvasProps = {
  /** Тот же холст, на котором `useProjectEngine` создал движок — требование Engine.create(canvas). */
  canvasRef: RefObject<HTMLCanvasElement | null>;
  engine: Engine | null;
  sceneSize: SceneSize | null;
  /**
   * Место и размер объекта по его номеру, для геометрии переноса (требование 8) — из текста
   * `scene.json` в правке, из живого мира в партии на паузе («Редактор», требование 20).
   */
  getObjectGeometry: (objectId: number) => ObjectGeometry | null;
  /** Ссылка меняется, когда объекты изменились под переносом извне — отменяет его на лету. */
  objectsVersion: unknown;
  /** `scene.json`/живой мир недоступны правке — перенос не начинается (крайний случай). */
  canEditScene: boolean;
  /** Мышь и клавиатура принадлежат игре, а не выбору/переносу — «Редактор», требование 4: партия идёт. */
  isGameInputActive: boolean;
  selectedIndex: number | null;
  /** Подпись над рамкой выбранного объекта — его имя или номер. */
  selectedLabel: string | null;
  onSelect: (index: number | null) => void;
  /**
   * Отпускание после переноса — «Редактор», требование 9: одно действие с новым местом объекта.
   * `previousPosition` — место на начало переноса, для отмены правки на ходу (требование 21): его
   * не восстановить из движка в момент отпускания — `move_object` уже успел передвинуть объект
   * там во время самого переноса, так что «текущее» свойство к этому моменту и есть новое место.
   */
  onMoveObject: (objectIndex: number, position: readonly [number, number], previousPosition: readonly [number, number]) => void;
};

type DragState = {
  objectIndex: number;
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startPosition: readonly [number, number];
  cellSizePx: number;
  hasStartedDrag: boolean;
  lastPosition: readonly [number, number];
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
export function SceneCanvas({
  canvasRef,
  engine,
  sceneSize,
  getObjectGeometry,
  objectsVersion,
  canEditScene,
  isGameInputActive,
  selectedIndex,
  selectedLabel,
  onSelect,
  onMoveObject,
}: SceneCanvasProps): React.JSX.Element {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const selectedIndexRef = useRef(selectedIndex);
  selectedIndexRef.current = selectedIndex;
  const selectedLabelRef = useRef(selectedLabel);
  selectedLabelRef.current = selectedLabel;
  const getObjectGeometryRef = useRef(getObjectGeometry);
  getObjectGeometryRef.current = getObjectGeometry;
  const canEditSceneRef = useRef(canEditScene);
  canEditSceneRef.current = canEditScene;
  const isGameInputActiveRef = useRef(isGameInputActive);
  isGameInputActiveRef.current = isGameInputActive;
  const onMoveObjectRef = useRef(onMoveObject);
  onMoveObjectRef.current = onMoveObject;
  const dragRef = useRef<DragState | null>(null);
  const sceneWidth = sceneSize?.width ?? null;
  const sceneHeight = sceneSize?.height ?? null;

  // Внешняя правка или другое действие поменяли объекты во время переноса — «Редактор», крайний
  // случай: перенос отменяется, мир движок уже собрал заново из показанного своей перезагрузкой.
  useEffect(() => {
    dragRef.current = null;
  }, [objectsVersion]);

  // Партия пошла или встала на паузу — начатый мышью перенос больше не имеет смысла в новом режиме.
  useEffect(() => {
    dragRef.current = null;
  }, [isGameInputActive]);

  // Клавиатура доходит до игры, только когда фокус на холсте, — «Редактор», требование 4.
  useEffect(() => {
    if (!isGameInputActive || !engine) return;
    const activeEngine = engine;
    const overlayCanvas = overlayCanvasRef.current;
    // Отпускание уходит игре, только если до неё дошло нажатие: отпускание сочетания редактора
    // (P от Ctrl+Shift+P) или клавиши, нажатой до фокуса на холсте, игре не принадлежит.
    const pressedCodes = new Set<string>();
    function handleKeyDown(event: KeyboardEvent): void {
      if (document.activeElement !== overlayCanvas) return;
      event.preventDefault();
      pressedCodes.add(event.code);
      activeEngine.key_down(event.code);
    }
    function handleKeyUp(event: KeyboardEvent): void {
      if (!pressedCodes.delete(event.code)) return;
      event.preventDefault();
      activeEngine.key_up(event.code);
    }
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [isGameInputActive, engine]);

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

  /**
   * Нажатие выбирает объект под указателем — «Редактор», требование 7 (выбор на нажатии, не на
   * отпускании). Правка недоступна или под указателем нет объекта с `position`/`size` в тексте —
   * перенос не заводится, но выбор всё равно работает (крайний случай: `scene.json` не разобрать).
   */
  function handlePointerDown(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (!engine || event.button !== 0) return;
    // Партия идёт — щелчок по холсту даёт ему фокус и уходит игре, а не выбору («Редактор», требование 4).
    if (isGameInputActiveRef.current) {
      event.currentTarget.focus();
      engine.mouse_down();
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const hit = engine.object_at(event.clientX - bounds.left, event.clientY - bounds.top) as number | undefined;
    // Открытое поле свойства записывается в прежний объект (требование 13) до смены выбора: иначе
    // панель пересоздаётся под новый номер раньше, чем браузер снимет фокус, и черновик пропадёт.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    onSelect(typeof hit === "number" ? hit : null);
    if (typeof hit !== "number" || !canEditSceneRef.current) return;

    const geometry = getObjectGeometryRef.current(hit);
    const rect = engine.object_rect(hit) as { width: number } | undefined;
    if (geometry === null || !rect) return;

    dragRef.current = {
      objectIndex: hit,
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startPosition: geometry.position,
      cellSizePx: cellSizeFromObjectRect(rect.width, geometry.size[0]),
      hasStartedDrag: false,
      lastPosition: geometry.position,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  /**
   * Пока партия идёт, указатель над холстом принадлежит игре целиком — «Редактор», требование 4:
   * `mouse_move` идёт независимо от того, зажата ли кнопка.
   */
  function handlePointerMove(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (!engine) return;
    if (isGameInputActiveRef.current) {
      const bounds = event.currentTarget.getBoundingClientRect();
      engine.mouse_move(event.clientX - bounds.left, event.clientY - bounds.top);
      return;
    }
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    const deltaX = event.clientX - drag.startClientX;
    const deltaY = event.clientY - drag.startClientY;
    if (!drag.hasStartedDrag) {
      // «Редактор», требование 7: сдвиг дальше 4 пикселей начинает перенос.
      if (!hasCrossedDragThreshold(deltaX, deltaY)) return;
      drag.hasStartedDrag = true;
    }
    const position = computeDragPosition(drag.startPosition, [deltaX, deltaY], drag.cellSizePx, event.ctrlKey);
    if (position[0] === drag.lastPosition[0] && position[1] === drag.lastPosition[1]) return;
    drag.lastPosition = position;
    engine.move_object(drag.objectIndex, position[0], position[1]);
  }

  function endDrag(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (isGameInputActiveRef.current) {
      engine?.mouse_up();
      return;
    }
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (drag.hasStartedDrag) onMoveObjectRef.current(drag.objectIndex, drag.lastPosition, drag.startPosition);
  }

  /** Браузер сам отменил перенос (например, второй палец на тачскрине) — объект возвращается на прежнее место, без действия. */
  function handlePointerCancel(event: React.PointerEvent<HTMLCanvasElement>): void {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (drag.hasStartedDrag) engine?.move_object(drag.objectIndex, drag.startPosition[0], drag.startPosition[1]);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLCanvasElement>): void {
    const drag = dragRef.current;
    if (event.key !== "Escape" || drag === null) return;
    dragRef.current = null;
    engine?.move_object(drag.objectIndex, drag.startPosition[0], drag.startPosition[1]);
  }

  function handleAreaClick(event: React.MouseEvent<HTMLDivElement>): void {
    if (event.target === event.currentTarget) onSelect(null);
  }

  return (
    <div ref={areaRef} className="scene-view" onClick={handleAreaClick}>
      <div ref={stageRef} className="scene-view__stage">
        <canvas ref={canvasRef} className="scene-view__world" />
        <canvas
          ref={overlayCanvasRef}
          className="scene-view__overlay"
          tabIndex={-1}
          data-game-input={isGameInputActive ? "true" : undefined}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={endDrag}
          onPointerCancel={handlePointerCancel}
          onKeyDown={handleKeyDown}
        />
      </div>
      {sceneSize !== null && (
        <span className="scene-view__size">
          Сцена {sceneSize.width} × {sceneSize.height}
        </span>
      )}
    </div>
  );
}
