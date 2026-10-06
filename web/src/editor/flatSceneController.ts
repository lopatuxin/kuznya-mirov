import type { Engine } from "engine";
import { hasCrossedDragThreshold } from "./dragPlacement";
import { focusFlatCameraOnObject, maxViewHeightFor, panFlatCamera, readFlatCamera, wheelClicks, zoomFlatCamera, type FlatCameraStore } from "./editorCamera";
import { computeFlatTranslateGeometry, hitTestFlatScaleHandles, type FlatScaleHandle } from "./flatHandles";
import { drawFlatScaleHandles, drawHandles } from "./handleDrawing";
import { hitTestHandles, type HandleHit, type HandleMode } from "./handleGeometry";
import { computeFlatScale, computeTranslate, type TranslateAxis } from "./handleMath";
import { diffPlacements, readObjectPlacement, type ObjectPlacement, type PlacementChange, type Vec2 } from "./objectPlacement";
import type { SceneSize } from "./sceneObjects";
import { drawSceneBoundary, drawSelection, type CanvasRect } from "./selectionDrawing";
import type { KeyInput, PointerInput } from "./spaceSceneController";
import { toVec2 } from "./terrainReadings";

/** Вызовы движка, которыми пользуется плоская сцена редактора. */
export type FlatSceneEngine = Pick<
  Engine,
  "object_at" | "object_rect" | "scene_point" | "screen_point" | "move_object" | "transform_object" | "editor_camera" | "fit_camera"
>;

/** Всё, что контроллер узнаёт у страницы при каждом событии, — свежее на этот миг. */
export type FlatSceneContext = {
  engine: FlatSceneEngine;
  cameraStore: FlatCameraStore;
  /** `null` — размера сцены нет: граница не рисуется. */
  sceneSize: SceneSize | null;
  /** Партия идёт: мышь и клавиши принадлежат игре («Редактор», требование 19). */
  isInputLocked: boolean;
  /** Сцена вне партии и повтора видна камерой редактора — её водят колесом, средней кнопкой и `F`. */
  isEditorCameraActive: boolean;
  /** Вне партии и на паузе, правка доступна: ручки, перенос, `W`, `R`. */
  areHandlesAvailable: boolean;
  selectedIndex: number | null;
  selectedLabel: string | null;
  /** `translate` или `scale`: поворота в плоской сцене нет. */
  handleMode: HandleMode;
  /** Свойства объекта в виде файла: из текста `scene.json` вне партии, из живого мира на паузе. */
  getObjectProperties: (objectId: number) => Record<string, unknown> | null;
  onSelect: (index: number | null) => void;
  onHandleModeChange: (mode: HandleMode) => void;
  /** Отпускание после жеста: изменившиеся `position` и `size` объекта — одно действие. */
  onCommitPlacement: (objectIndex: number, changes: PlacementChange[]) => void;
};

export type FlatWheelInput = { deltaY: number; deltaMode: number; ctrlKey: boolean; x: number; y: number };

type FlatHit = HandleHit | FlatScaleHandle;

type PlacementGesture = {
  kind: "handle" | "body";
  pointerId: number;
  button: number;
  objectIndex: number;
  hit: FlatHit | null;
  mode: "translate" | "scale";
  start: ObjectPlacement;
  last: ObjectPlacement;
  /** `parallax` объекта: по нему точка под указателем переводится в записанное место. */
  parallax: number;
  startPoint: Vec2;
  /** Место сцены под указателем в начале жеста — сдвиг считается от него по нынешней камере. */
  startScene: Vec2;
  hasStarted: boolean;
};

type Gesture = PlacementGesture | { kind: "pan"; pointerId: number; button: number; grabbed: Vec2 };

const MIDDLE_BUTTON = 1;
const LEFT_BUTTON = 0;

// `PointerEvent.buttons`: отпущена другая кнопка, пока эта ещё зажата, — жест продолжается.
const BUTTON_BITS: Record<number, number> = { 0: 1, 1: 4 };

const FLAT_MODE_KEYS: Record<string, HandleMode> = { KeyW: "translate", KeyR: "scale" };

const TRANSLATE_AXES: Partial<Record<FlatHit, TranslateAxis>> = { "axis-x": "x", "axis-y": "y" };

export type FlatSceneController = {
  pointerDown: (input: PointerInput) => boolean;
  pointerMove: (input: PointerInput) => void;
  pointerUp: (input: PointerInput) => void;
  pointerCancel: (input: PointerInput) => void;
  pointerLeave: () => void;
  wheel: (input: FlatWheelInput) => boolean;
  keyDown: (input: KeyInput) => boolean;
  /** Граница сцены, рамка выбранного объекта и его ручки поверх холста. */
  draw: (context: CanvasRenderingContext2D, pixelRatio: number) => void;
  /** Жест бросается без возврата объекта: мир уже собран заново (правка файла снаружи, партия пошла). */
  abandonGesture: () => void;
};

function parallaxOf(properties: Record<string, unknown> | null): number {
  const parallax = properties?.parallax;
  return typeof parallax === "number" && Number.isFinite(parallax) ? parallax : 1;
}

/** Ставит объекту в собранном мире значения жеста: только место — `move_object`, вместе с размером — `transform_object`. */
function sendPlacement(engine: FlatSceneEngine, objectId: number, mode: "translate" | "scale", placement: ObjectPlacement): void {
  if (mode === "translate") {
    engine.move_object(objectId, placement.position[0], placement.position[1]);
    return;
  }
  engine.transform_object(objectId, { position: [placement.position[0], placement.position[1]], size: [placement.size[0], placement.size[1]] });
}

/**
 * Мышь и клавиши плоской сцены редактора — «Редактор», «Сцена» и «Правка сцены»: камера редактора (колесо,
 * средняя кнопка, `F`), выбор щелчком, ручки переноса и масштаба, перенос самого объекта, `Esc`. Сам с DOM не
 * связан — страница отдаёт события как есть и свежий `FlatSceneContext` по требованию.
 */
export function createFlatSceneController(getContext: () => FlatSceneContext): FlatSceneController {
  let gesture: Gesture | null = null;
  let hovered: FlatHit | null = null;

  function scenePoint(engine: FlatSceneEngine, point: Vec2, parallax: number): Vec2 | undefined {
    return toVec2(engine.scene_point(point[0], point[1], parallax));
  }

  function selectedRect(context: FlatSceneContext): CanvasRect | undefined {
    return context.selectedIndex === null ? undefined : (context.engine.object_rect(context.selectedIndex) as CanvasRect | undefined);
  }

  function modeOf(context: FlatSceneContext): "translate" | "scale" {
    return context.handleMode === "scale" ? "scale" : "translate";
  }

  function hitTestSelected(context: FlatSceneContext, rect: CanvasRect, point: Vec2): FlatHit | null {
    if (modeOf(context) === "scale") return hitTestFlatScaleHandles(rect, point);
    return hitTestHandles(computeFlatTranslateGeometry(rect), "translate", point);
  }

  function startPlacementGesture(context: FlatSceneContext, input: PointerInput, objectIndex: number, placement: ObjectPlacement, hit: FlatHit | null): boolean {
    const point: Vec2 = [input.x, input.y];
    const parallax = parallaxOf(context.getObjectProperties(objectIndex));
    const startScene = scenePoint(context.engine, point, parallax);
    if (startScene === undefined) return false;
    gesture = {
      kind: hit === null ? "body" : "handle",
      pointerId: input.pointerId,
      button: input.button,
      objectIndex,
      hit,
      mode: hit === null ? "translate" : modeOf(context),
      start: placement,
      last: placement,
      parallax,
      startPoint: point,
      startScene,
      hasStarted: hit !== null,
    };
    return true;
  }

  function pointerDown(input: PointerInput): boolean {
    const context = getContext();
    if (context.isInputLocked || gesture !== null) return false;
    const point: Vec2 = [input.x, input.y];
    if (input.button === MIDDLE_BUTTON) {
      if (!context.isEditorCameraActive || context.cameraStore.current() === null) return false;
      const grabbed = scenePoint(context.engine, point, 1);
      if (grabbed === undefined) return false;
      gesture = { kind: "pan", pointerId: input.pointerId, button: input.button, grabbed };
      return true;
    }
    if (input.button !== LEFT_BUTTON) return false;

    // Ручка под указателем важнее самого объекта («Правка сцены», требование 21).
    if (context.areHandlesAvailable && context.selectedIndex !== null) {
      const rect = selectedRect(context);
      const placement = readObjectPlacement(context.getObjectProperties(context.selectedIndex));
      const hit = rect === undefined ? null : hitTestSelected(context, rect, point);
      if (placement !== null && hit !== null) return startPlacementGesture(context, input, context.selectedIndex, placement, hit);
    }

    const picked = context.engine.object_at(input.x, input.y) as number | undefined;
    if (typeof picked !== "number") {
      context.onSelect(null);
      return false;
    }
    context.onSelect(picked);
    if (!context.areHandlesAvailable) return false;
    const placement = readObjectPlacement(context.getObjectProperties(picked));
    return placement !== null && startPlacementGesture(context, input, picked, placement, null);
  }

  function applyPlacementGesture(context: FlatSceneContext, active: PlacementGesture, input: PointerInput): void {
    const point: Vec2 = [input.x, input.y];
    if (!active.hasStarted) {
      if (!hasCrossedDragThreshold(point[0] - active.startPoint[0], point[1] - active.startPoint[1])) return;
      active.hasStarted = true;
    }
    const now = scenePoint(context.engine, point, active.parallax);
    if (now === undefined) return;
    const delta: Vec2 = [now[0] - active.startScene[0], now[1] - active.startScene[1]];
    const start = active.start;
    if (active.mode === "scale") {
      const scaled = computeFlatScale(start, active.hit as FlatScaleHandle, delta, input.ctrlKey);
      active.last = { ...start, position: scaled.position, size: scaled.size };
    } else {
      const axis = active.hit === null ? "free" : (TRANSLATE_AXES[active.hit] ?? "free");
      active.last = { ...start, position: computeTranslate(start.position, delta, axis, input.ctrlKey) };
    }
    sendPlacement(context.engine, active.objectIndex, active.mode, active.last);
  }

  function updateHover(context: FlatSceneContext, input: PointerInput): void {
    hovered = null;
    if (!context.areHandlesAvailable) return;
    const rect = selectedRect(context);
    if (rect !== undefined) hovered = hitTestSelected(context, rect, [input.x, input.y]);
  }

  function pointerMove(input: PointerInput): void {
    const context = getContext();
    if (context.isInputLocked) return;
    const active = gesture;
    if (active === null) {
      updateHover(context, input);
      return;
    }
    if (active.pointerId !== input.pointerId) return;
    if (active.kind === "pan") {
      const camera = context.cameraStore.current();
      const under = scenePoint(context.engine, [input.x, input.y], 1);
      if (camera !== null && under !== undefined) context.cameraStore.update(context.engine, panFlatCamera(camera, active.grabbed, under));
      return;
    }
    applyPlacementGesture(context, active, input);
  }

  function pointerUp(input: PointerInput): void {
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId || (input.buttons & BUTTON_BITS[active.button]) !== 0) return;
    gesture = null;
    if (active.kind === "pan" || !active.hasStarted) return;
    const changes = diffPlacements(active.start, active.last);
    if (changes.length > 0) getContext().onCommitPlacement(active.objectIndex, changes);
  }

  function restoreStart(active: PlacementGesture): void {
    if (active.hasStarted) sendPlacement(getContext().engine, active.objectIndex, active.mode, active.start);
  }

  function pointerCancel(input: PointerInput): void {
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId) return;
    gesture = null;
    if (active.kind !== "pan") restoreStart(active);
  }

  function wheel(input: FlatWheelInput): boolean {
    const context = getContext();
    if (context.isInputLocked || !context.isEditorCameraActive) return false;
    const camera = context.cameraStore.current();
    const wholeScene = readFlatCamera(context.engine.fit_camera(null));
    const under = scenePoint(context.engine, [input.x, input.y], 1);
    if (camera === null || wholeScene === undefined || under === undefined) return false;
    const zoomed = zoomFlatCamera(camera, wheelClicks(input.deltaY, input.deltaMode), under, maxViewHeightFor(wholeScene.view_height));
    context.cameraStore.update(context.engine, zoomed);
    return true;
  }

  function keyDown(input: KeyInput): boolean {
    const context = getContext();
    if (context.isInputLocked || input.ctrlKey || input.altKey || input.metaKey) return false;
    if (input.code === "Escape") {
      const active = gesture;
      if (active === null || active.kind === "pan") return false;
      gesture = null;
      restoreStart(active);
      return true;
    }
    const mode = FLAT_MODE_KEYS[input.code];
    if (mode !== undefined && !input.shiftKey) {
      if (!context.areHandlesAvailable || gesture !== null) return false;
      hovered = null;
      context.onHandleModeChange(mode);
      return true;
    }
    if (input.code === "KeyF" && !input.shiftKey) {
      if (!context.isEditorCameraActive) return false;
      if (context.selectedIndex !== null) focusFlatCameraOnObject(context.cameraStore, context.engine, context.selectedIndex);
      return true;
    }
    return false;
  }

  function drawBoundary(context: FlatSceneContext, context2d: CanvasRenderingContext2D, pixelRatio: number): void {
    if (!context.isEditorCameraActive || context.sceneSize === null) return;
    const from = toVec2(context.engine.screen_point(0, 0, 0));
    const to = toVec2(context.engine.screen_point(context.sceneSize.width, context.sceneSize.height, 0));
    if (from !== undefined && to !== undefined) drawSceneBoundary(context2d, from, to, pixelRatio);
  }

  function draw(context2d: CanvasRenderingContext2D, pixelRatio: number): void {
    const context = getContext();
    drawBoundary(context, context2d, pixelRatio);
    const rect = selectedRect(context);
    if (rect === undefined) return;
    drawSelection(context2d, rect, context.selectedLabel, pixelRatio);
    if (!context.areHandlesAvailable) return;
    if (modeOf(context) === "scale") {
      drawFlatScaleHandles(context2d, rect, hovered as FlatScaleHandle | null, pixelRatio);
      return;
    }
    drawHandles(context2d, computeFlatTranslateGeometry(rect), "translate", hovered as HandleHit | null, pixelRatio);
  }

  return {
    pointerDown,
    pointerMove,
    pointerUp,
    pointerCancel,
    pointerLeave: () => {
      hovered = null;
    },
    wheel,
    keyDown,
    draw,
    abandonGesture: () => {
      gesture = null;
    },
  };
}
