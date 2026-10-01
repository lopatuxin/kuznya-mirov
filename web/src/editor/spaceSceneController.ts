import type { Engine } from "engine";
import { brushRingPoints, drawBrushCircle } from "./brushCircle";
import { focusCameraOnObject, orbitCamera, panCamera, wheelClicks, zoomCamera, type EditorCameraStore } from "./editorCamera";
import { drawHandles } from "./handleDrawing";
import {
  ARROW_LENGTH_PX,
  arrowVectorOfHit,
  computeHandleGeometry,
  hitTestHandles,
  scaleAxisOfHit,
  type HandleGeometry,
  type HandleHit,
  type HandleMode,
  type SpaceProjection,
} from "./handleGeometry";
import { computeHeight, computeRotation, computeScale, computeTranslate, scaleFactorAlong, scaleFactorUniform, type ScaleAxis, type TranslateAxis } from "./handleMath";
import {
  DEFAULT_SHAPE_HEIGHT,
  diffPlacements,
  effectiveHeight,
  placementCenter,
  readObjectPlacement,
  type ObjectPlacement,
  type PlacementChange,
  type Vec2,
} from "./objectPlacement";
import { createMountainSceneController, type MountainContext } from "./mountainSceneController";
import { drawSelectionQuad } from "./selectionDrawing";
import { applyBrushFrame, brushPathPoints, type BrushGrid, type BrushSettings } from "./terrainBrush";
import { differsInHundredths, type MountainEntry, type TerrainGrid } from "./terrainFile";
import { readTerrainSnapshot, toVec2, toVec3, type Vec3 } from "./terrainReadings";
import { createVerticalGrab, type VerticalGrab } from "./verticalGrab";

/** Вызовы движка, которыми пользуется трёхмерная сцена редактора. */
export type SpaceSceneEngine = Pick<
  Engine,
  | "object_at"
  | "object_rect"
  | "object_properties"
  | "ground_at"
  | "rest_height"
  | "screen_point"
  | "move_object"
  | "transform_object"
  | "editor_camera"
  | "fit_camera"
  | "terrain_at"
  | "terrain_height"
  | "terrain_heights"
  | "set_terrain"
  | "stamp_at"
>;

/** Всё, что контроллер узнаёт у страницы при каждом событии, — свежее на этот миг. */
export type SpaceSceneContext = {
  engine: SpaceSceneEngine;
  cameraStore: EditorCameraStore;
  /** Партия идёт: мышь и клавиши принадлежат игре («Редактор», требование 19). */
  isInputLocked: boolean;
  /** Сцена вне партии и повтора видна камерой редактора — её водят мышью. */
  isEditorCameraActive: boolean;
  /** Вне партии и на паузе, правка доступна: ручки, перенос, `W`, `E`, `R`. */
  areHandlesAvailable: boolean;
  selectedIndex: number | null;
  selectedLabel: string | null;
  handleMode: HandleMode;
  /** Выбрана кисть рельефа — левая кнопка лепит землю, ручек нет; `null` — выбраны ручки или кисти недоступны. */
  brush: BrushSettings | null;
  /** Свойства объекта в виде файла: из текста `scene.json` вне партии, из живого мира на паузе. */
  getObjectProperties: (objectId: number) => Record<string, unknown> | null;
  onSelect: (index: number | null) => void;
  onHandleModeChange: (mode: HandleMode) => void;
  /** Отпускание после жеста: изменившиеся свойства объекта — одно действие («Редактор», требование 15). */
  onCommitPlacement: (objectIndex: number, changes: PlacementChange[]) => void;
  /** Отпускание после мазка: высоты всей сетки — одно действие («Кисти рельефа», требование 18). */
  onCommitTerrain: (grid: TerrainGrid) => void;
  /** Мазок начался или кончился любым способом — пока он идёт, изменения файлов снаружи ждут («Кисти рельефа», крайние случаи). */
  onStrokeActiveChange: (isActive: boolean) => void;
  /** Горы файла рельефа: выбор, ручки, кнопка «Гора» («Лепка рельефа», «Редактор»). */
  mountains: MountainContext;
};

/** Указатель или колесо: точка в CSS-пикселях от левого верхнего угла холста. */
export type PointerInput = { pointerId: number; button: number; buttons: number; x: number; y: number; ctrlKey: boolean; shiftKey: boolean; timeStamp: number };
type WheelInput = { deltaY: number; deltaMode: number };
export type KeyInput = { code: string; ctrlKey: boolean; altKey: boolean; metaKey: boolean; shiftKey: boolean };

/** Каким вызовом жест ставит объекту значения: только место, поворот, размеры или размеры с высотой. */
type PlacementChannel = "position" | "rotation" | "size" | "size-and-height";

type ObjectGesture = {
  kind: "handle" | "body";
  pointerId: number;
  button: number;
  objectIndex: number;
  hit: HandleHit | null;
  mode: HandleMode;
  channel: PlacementChannel;
  start: ObjectPlacement;
  last: ObjectPlacement;
  /** Высота основания на начало жеста и как её поставил последний вызов движка («Рельеф», требования 41–44). */
  startHeight: number;
  lastHeight: number;
  startPoint: Vec2;
  startGround: Vec3 | null;
  /** Вертикальная стрелка: вертикаль через середину, как её видит камера на начало жеста. */
  verticalGrab: VerticalGrab | null;
  /** Ручки на начало жеста — по ним считается доля масштаба. */
  startGeometry: HandleGeometry | null;
  hasStarted: boolean;
};

/**
 * Мазок кисти рельефа: кисть лепит итоговую землю (`grid` — итоговые высоты), а в файл идут высоты без гор
 * (`heights`) — «Лепка рельефа», требование 20: `heights` плюс то, на сколько мазок поднял итоговую землю.
 */
type StrokeGesture = {
  kind: "stroke";
  pointerId: number;
  button: number;
  settings: BrushSettings;
  pointer: Vec2;
  isLowering: boolean;
  lastFrameMs: number;
  /** Точка кисти прошлого кадра; `null` — указатель ушёл с земли, путь до новой точки не тянется. */
  lastPoint: Vec2 | null;
  /** «Выровнять»: высота рельефа в точке нажатия. */
  levelTarget: number;
  grid: BrushGrid;
  startEffective: Float64Array;
  heights: Float64Array;
  start: Float64Array;
  water: unknown;
  /** Горы файла на начало мазка: он их не меняет, но `set_terrain` получает их каждый кадр. */
  stamps: readonly MountainEntry[];
  hasChanged: boolean;
  /** Кому сказать о конце мазка — запомнен при нажатии: к концу мазка контекста сцены может уже не быть. */
  onActiveChange: (isActive: boolean) => void;
};

type Gesture =
  | ObjectGesture
  | StrokeGesture
  | { kind: "orbit"; pointerId: number; button: number; lastX: number; lastY: number }
  | { kind: "pan"; pointerId: number; button: number; grabbed: Vec2 };

/** «Редактор», требование 14: сдвиг указателя дальше 4 точек начинает перенос объекта. */
const BODY_DRAG_THRESHOLD_PX = 4;
/** «Рельеф», требование 44: высота, что движок поставил бы без `z`, и высота объекта совпадают с точностью до этого. */
const HEIGHT_TOLERANCE = 0.005;
const MIDDLE_BUTTON = 1;
const LEFT_BUTTON = 0;

// `PointerEvent.buttons`: отпущена другая кнопка, пока эта ещё зажата, — жест продолжается.
const BUTTON_BITS: Record<number, number> = { 0: 1, 1: 4 };

const MODE_KEYS: Record<string, HandleMode> = { KeyW: "translate", KeyE: "rotate", KeyR: "scale" };

function isObjectGesture(gesture: Gesture | null): gesture is ObjectGesture {
  return gesture !== null && (gesture.kind === "handle" || gesture.kind === "body");
}

function isStrokeGesture(gesture: Gesture | null): gesture is StrokeGesture {
  return gesture !== null && gesture.kind === "stroke";
}

function restHeight(engine: SpaceSceneEngine, objectId: number, x: number, y: number, from?: number): number | undefined {
  const height = engine.rest_height(objectId, x, y, from);
  return typeof height === "number" ? height : undefined;
}

/** Высота основания, на которой объект стоит в собранном мире, — третье число его `position`. */
function worldHeight(engine: SpaceSceneEngine, objectId: number): number | undefined {
  const properties = engine.object_properties(objectId) as { position?: unknown } | undefined;
  const height = Array.isArray(properties?.position) ? (properties.position as unknown[])[2] : undefined;
  return typeof height === "number" ? height : undefined;
}

/** Высота основания объекта в покое: записанная или та, на которую он садится без `z` («Рельеф», требование 8). */
function restingHeight(engine: SpaceSceneEngine, objectId: number, placement: ObjectPlacement): number {
  return placement.z ?? restHeight(engine, objectId, placement.position[0], placement.position[1]) ?? 0;
}

function roundToHundredth(value: number): number {
  return Math.round(value * 100) / 100;
}

/**
 * Ставит объекту в собранном мире значения жеста — `move_object` для места, `transform_object` для
 * остального. `height` — основание ровно на нём; `null` — объект садится на поверхность сам
 * («Рельеф», требование 10).
 */
function sendPlacement(engine: SpaceSceneEngine, objectId: number, channel: PlacementChannel, placement: ObjectPlacement, height: number | null): void {
  if (channel === "position") {
    engine.move_object(objectId, placement.position[0], placement.position[1], height);
    return;
  }
  engine.transform_object(objectId, {
    position: height === null ? [placement.position[0], placement.position[1]] : [placement.position[0], placement.position[1], height],
    size: [placement.size[0], placement.size[1]],
    ...(channel === "rotation" ? { rotation: placement.rotation ?? 0 } : {}),
    ...(channel === "size-and-height" ? { height: effectiveHeight(placement) ?? DEFAULT_SHAPE_HEIGHT } : {}),
  });
}

export type SpaceSceneController = {
  pointerDown: (input: PointerInput) => boolean;
  pointerMove: (input: PointerInput) => void;
  pointerUp: (input: PointerInput) => void;
  pointerCancel: (input: PointerInput) => void;
  pointerLeave: () => void;
  wheel: (input: WheelInput) => boolean;
  keyDown: (input: KeyInput) => boolean;
  keyUp: (input: KeyInput) => void;
  /** Кадр страницы: пока кнопка нажата, кисть действует, даже если указатель стоит; `nowMs` — время кадра. */
  strokeFrame: (nowMs: number) => void;
  /** Рамка выбранного объекта, его ручки и круг кисти поверх холста. */
  draw: (context: CanvasRenderingContext2D, pixelRatio: number) => void;
  /** Жест бросается без возврата объекта: мир уже собран заново (правка файла снаружи, партия пошла). */
  abandonGesture: () => void;
};

/**
 * Мышь и клавиши трёхмерной сцены редактора — «Редактор», «Сцена» и «Правка сцены»: камера
 * редактора (средняя кнопка, Shift, колесо, `F`), выбор щелчком, ручки переноса, поворота и масштаба,
 * перенос самого объекта по земле, `Esc`. Сам с DOM не связан — страница отдаёт события как есть и
 * свежий `SpaceSceneContext` по требованию.
 */
export function createSpaceSceneController(getContext: () => SpaceSceneContext): SpaceSceneController {
  let gesture: Gesture | null = null;
  let hovered: HandleHit | null = null;
  /** Где указатель над холстом — по нему кисть рисует круг, пока кнопка не нажата. */
  let hoverPoint: Vec2 | null = null;
  const mountainController = createMountainSceneController();

  function projectionOf(engine: SpaceSceneEngine): SpaceProjection {
    return { screenPoint: (x, y, z) => toVec2(engine.screen_point(x, y, z)) };
  }

  function groundAt(engine: SpaceSceneEngine, point: Vec2): Vec3 | undefined {
    return toVec3(engine.ground_at(point[0], point[1]));
  }

  /** Выбранный объект и высота его основания: во время жеста — как её поставил движок, иначе — в покое. */
  function selectedHandleSubject(context: SpaceSceneContext): { placement: ObjectPlacement; baseHeight: number } | null {
    if (context.selectedIndex === null) return null;
    if (isObjectGesture(gesture) && gesture.objectIndex === context.selectedIndex) return { placement: gesture.last, baseHeight: gesture.lastHeight };
    const placement = readObjectPlacement(context.getObjectProperties(context.selectedIndex));
    return placement === null ? null : { placement, baseHeight: restingHeight(context.engine, context.selectedIndex, placement) };
  }

  function startObjectGesture(
    context: SpaceSceneContext,
    input: PointerInput,
    objectIndex: number,
    placement: ObjectPlacement,
    hit: HandleHit | null,
    geometry: HandleGeometry | null,
  ): boolean {
    const point: Vec2 = [input.x, input.y];
    const startGround = groundAt(context.engine, point) ?? null;
    const mode: HandleMode = hit === null ? "translate" : context.handleMode;
    const startHeight = restingHeight(context.engine, objectIndex, placement);
    const isVertical = mode === "translate" && hit === "axis-z";
    let channel: PlacementChannel = "position";
    if (mode === "rotate") channel = "rotation";
    if (mode === "scale") {
      const axis = scaleAxisOfHit(hit ?? "center");
      channel = (axis === "height" || axis === "uniform") && placement.hasShape ? "size-and-height" : "size";
    }
    if (mode !== "scale" && !isVertical && startGround === null) return false;
    const verticalGrab = isVertical ? (createVerticalGrab(projectionOf(context.engine), placementCenter(placement), startHeight) ?? null) : null;
    if (isVertical && verticalGrab === null) return false;
    gesture = {
      kind: hit === null ? "body" : "handle",
      pointerId: input.pointerId,
      button: input.button,
      objectIndex,
      hit,
      mode,
      channel,
      start: placement,
      last: placement,
      startHeight,
      lastHeight: startHeight,
      startPoint: point,
      startGround,
      verticalGrab,
      startGeometry: geometry,
      hasStarted: hit !== null,
    };
    return true;
  }

  /** Нажатие при выбранной кисти — «Кисти рельефа», требования 6, 8: нет точки кисти под указателем — мазка нет. */
  function startStroke(context: SpaceSceneContext, input: PointerInput, settings: BrushSettings): boolean {
    const place = toVec3(context.engine.terrain_at(input.x, input.y));
    const terrain = readTerrainSnapshot(context.engine.terrain_heights());
    if (place === undefined || terrain === undefined) return false;
    gesture = {
      kind: "stroke",
      pointerId: input.pointerId,
      button: input.button,
      settings,
      pointer: [input.x, input.y],
      isLowering: input.shiftKey,
      lastFrameMs: input.timeStamp,
      lastPoint: [place[0], place[1]],
      levelTarget: place[2],
      grid: { ...terrain.grid, heights: Float64Array.from(terrain.effective) },
      startEffective: terrain.effective,
      heights: Float64Array.from(terrain.grid.heights),
      start: terrain.grid.heights,
      water: terrain.water,
      stamps: context.mountains.entries,
      hasChanged: false,
      onActiveChange: context.onStrokeActiveChange,
    };
    context.onStrokeActiveChange(true);
    return true;
  }

  function sendTerrain(engine: SpaceSceneEngine, heights: Float64Array, water: unknown, stamps: readonly MountainEntry[]): boolean {
    return engine.set_terrain(heights, water, stamps) === undefined;
  }

  /** Высоты файла после кадра мазка: прежние плюс то, на сколько кисть изменила итоговую землю. */
  function syncFileHeights(active: StrokeGesture): void {
    for (let index = 0; index < active.heights.length; index += 1) {
      active.heights[index] = (active.start[index] as number) + (active.grid.heights[index] as number) - (active.startEffective[index] as number);
    }
  }

  function strokeFrame(nowMs: number): void {
    const active = gesture;
    if (!isStrokeGesture(active)) return;
    const context = getContext();
    const seconds = (nowMs - active.lastFrameMs) / 1000;
    active.lastFrameMs = nowMs;
    const place = toVec3(context.engine.terrain_at(active.pointer[0], active.pointer[1]));
    if (place === undefined) {
      active.lastPoint = null;
      return;
    }
    const point: Vec2 = [place[0], place[1]];
    const path = brushPathPoints(active.lastPoint, point, active.settings.size);
    active.lastPoint = point;
    if (seconds <= 0) return;
    applyBrushFrame(active.grid, path, { settings: active.settings, seconds, isLowering: active.isLowering, levelTarget: active.levelTarget });
    active.hasChanged = true;
    syncFileHeights(active);
    if (sendTerrain(context.engine, active.heights, active.water, active.stamps)) return;
    // Движок отказал (игра пересобрана или пошла партия) — мазок бросается без действия.
    gesture = null;
    active.onActiveChange(false);
  }

  /** Рельеф до мазка — Esc, сброс указателя: файл не пишется, действия нет («Кисти рельефа», требование 16). */
  function undoStroke(active: StrokeGesture): void {
    if (active.hasChanged) sendTerrain(getContext().engine, active.start, active.water, active.stamps);
    active.onActiveChange(false);
  }

  /** Отпускание — одно действие; ни одна высота не изменилась в сотых — не действие, рельеф как был (требование 18). */
  function finishStroke(active: StrokeGesture): void {
    if (!differsInHundredths(active.start, active.heights)) {
      undoStroke(active);
      return;
    }
    getContext().onCommitTerrain({ columns: active.grid.columns, rows: active.grid.rows, heights: active.heights });
    active.onActiveChange(false);
  }

  function pointerDown(input: PointerInput): boolean {
    const context = getContext();
    if (context.isInputLocked || gesture !== null || mountainController.isActive()) return false;
    if (input.button === MIDDLE_BUTTON) {
      const camera = context.cameraStore.current();
      if (!context.isEditorCameraActive || camera === null) return false;
      if (!input.shiftKey) {
        gesture = { kind: "orbit", pointerId: input.pointerId, button: input.button, lastX: input.x, lastY: input.y };
        return true;
      }
      const grabbed = groundAt(context.engine, [input.x, input.y]);
      if (grabbed === undefined) return false;
      gesture = { kind: "pan", pointerId: input.pointerId, button: input.button, grabbed: [grabbed[0], grabbed[1]] };
      return true;
    }
    if (input.button !== LEFT_BUTTON) return false;
    // Кисть: нажатие не выбирает объект и выбор не снимает («Кисти рельефа», требование 8).
    if (context.brush !== null) return startStroke(context, input, context.brush);
    // «Гора»: щелчок по земле ставит гору и сам ничего не выбирает и не тянет.
    if (context.mountains.placing !== null) {
      mountainController.place(context, input);
      return false;
    }

    const point: Vec2 = [input.x, input.y];
    if (context.areHandlesAvailable && context.selectedIndex !== null) {
      const placement = readObjectPlacement(context.getObjectProperties(context.selectedIndex));
      const baseHeight = placement === null ? 0 : restingHeight(context.engine, context.selectedIndex, placement);
      const geometry = placement === null ? undefined : computeHandleGeometry(projectionOf(context.engine), placement, context.handleMode, baseHeight);
      const hit = geometry === undefined ? null : hitTestHandles(geometry, context.handleMode, point);
      if (placement !== null && geometry !== undefined && hit !== null) {
        return startObjectGesture(context, input, context.selectedIndex, placement, hit, geometry);
      }
    }

    if (mountainController.startHandle(context, input)) return true;

    // Объект важнее горы: щелчок по объекту, что стоит на горе, выбирает объект («Лепка рельефа», «Сцена»).
    const picked = context.engine.object_at(input.x, input.y) as number | undefined;
    if (typeof picked !== "number") {
      const mountainResult = mountainController.pickAt(context, input);
      if (mountainResult === null) context.onSelect(null);
      return mountainResult === true;
    }
    context.onSelect(picked);
    if (!context.areHandlesAvailable) return false;
    const placement = readObjectPlacement(context.getObjectProperties(picked));
    if (placement === null) return false;
    return startObjectGesture(context, input, picked, placement, null, null);
  }

  function applyObjectGesture(context: SpaceSceneContext, active: ObjectGesture, input: PointerInput): void {
    const point: Vec2 = [input.x, input.y];
    if (!active.hasStarted) {
      if (Math.hypot(point[0] - active.startPoint[0], point[1] - active.startPoint[1]) <= BODY_DRAG_THRESHOLD_PX) return;
      active.hasStarted = true;
    }
    const next = nextPlacement(context, active, point, input.ctrlKey);
    if (next === null) return;
    active.last = next.placement;
    sendPlacement(context.engine, active.objectIndex, active.channel, next.placement, next.height);
    active.lastHeight = next.height ?? worldHeight(context.engine, active.objectIndex) ?? active.lastHeight;
  }

  /**
   * Новые значения жеста и высота основания, на которую их надо поставить («Рельеф», требования
   * 41–43): перенос за тело садит объект так, будто он сдвинулся с высоты точки под указателем;
   * стрелки `x` и `y` — с высоты начала жеста; вертикальная стрелка меняет одну высоту; поворот и
   * масштаб высоту не задают (`null`) — объект садится сам.
   */
  function nextPlacement(
    context: SpaceSceneContext,
    active: ObjectGesture,
    point: Vec2,
    ctrlKey: boolean,
  ): { placement: ObjectPlacement; height: number | null } | null {
    const start = active.start;
    if (active.mode === "translate" && active.hit === "axis-z") {
      if (active.verticalGrab === null) return null;
      return { placement: start, height: computeHeight(active.startHeight, active.verticalGrab, active.startPoint, point, ctrlKey) };
    }
    if (active.mode === "scale") {
      const axis = scaleAxisOfHit(active.hit ?? "center") as ScaleAxis;
      const geometry = active.startGeometry;
      if (geometry === null) return null;
      const arrow = active.hit === null ? null : arrowVectorOfHit(geometry, active.hit);
      const factor = axis === "uniform" || arrow === null ? scaleFactorUniform(active.startPoint, point, ARROW_LENGTH_PX) : scaleFactorAlong(geometry.center, arrow, active.startPoint, point);
      return { placement: computeScale(start, axis, factor, ctrlKey), height: null };
    }
    const ground = groundAt(context.engine, point);
    if (ground === undefined || active.startGround === null) return null;
    if (active.mode === "rotate") {
      const turned = computeRotation(start.rotation, placementCenter(start), [active.startGround[0], active.startGround[1]], [ground[0], ground[1]], ctrlKey);
      return { placement: { ...start, rotation: turned }, height: null };
    }
    const delta: Vec2 = [ground[0] - active.startGround[0], ground[1] - active.startGround[1]];
    let axis: TranslateAxis = "free";
    if (active.hit === "axis-x") axis = "x";
    if (active.hit === "axis-y") axis = "y";
    const position = computeTranslate(start.position, delta, axis, ctrlKey);
    const fromHeight = axis === "free" ? ground[2] : active.startHeight;
    const height = restHeight(context.engine, active.objectIndex, position[0], position[1], fromHeight) ?? active.lastHeight;
    return { placement: { ...start, position }, height };
  }

  function updateHover(context: SpaceSceneContext, input: PointerInput): void {
    hovered = null;
    if (!context.areHandlesAvailable || context.brush !== null) return;
    const subject = selectedHandleSubject(context);
    if (subject === null) return;
    const geometry = computeHandleGeometry(projectionOf(context.engine), subject.placement, context.handleMode, subject.baseHeight);
    if (geometry !== undefined) hovered = hitTestHandles(geometry, context.handleMode, [input.x, input.y]);
  }

  function pointerMove(input: PointerInput): void {
    const context = getContext();
    if (context.isInputLocked) return;
    hoverPoint = [input.x, input.y];
    if (mountainController.pointerMove(context, input)) return;
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId) {
      if (active === null) updateHover(context, input);
      return;
    }
    if (isStrokeGesture(active)) {
      active.pointer = [input.x, input.y];
      active.isLowering = input.shiftKey;
      return;
    }
    if (active.kind === "orbit") {
      const camera = context.cameraStore.current();
      if (camera !== null) context.cameraStore.update(context.engine, orbitCamera(camera.camera, input.x - active.lastX, input.y - active.lastY));
      active.lastX = input.x;
      active.lastY = input.y;
      return;
    }
    if (active.kind === "pan") {
      const camera = context.cameraStore.current();
      const under = groundAt(context.engine, [input.x, input.y]);
      if (camera !== null && under !== undefined) context.cameraStore.update(context.engine, panCamera(camera.camera, active.grabbed, [under[0], under[1]]));
      return;
    }
    applyObjectGesture(context, active, input);
  }

  function pointerUp(input: PointerInput): void {
    if (mountainController.pointerUp(getContext(), input)) return;
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId || (input.buttons & BUTTON_BITS[active.button]) !== 0) return;
    gesture = null;
    if (isStrokeGesture(active)) {
      finishStroke(active);
      return;
    }
    if (!isObjectGesture(active) || !active.hasStarted) return;
    const context = getContext();
    const changes = diffPlacements(active.start, placementToWrite(context, active));
    if (changes.length > 0) context.onCommitPlacement(active.objectIndex, changes);
  }

  /**
   * Что жест пишет в `position` — «Рельеф», требование 44: ни место, ни высота не сменились — как
   * было записано. Иначе на паузе партии — всегда три числа; вне партии движок спрашивается, на какую
   * высоту он поставил бы объект без `z` на новом месте: совпало с высотой объекта — два числа
   * (третье, если было, убирается), нет — три, `z` до сотой клетки.
   */
  function placementToWrite(context: SpaceSceneContext, active: ObjectGesture): ObjectPlacement {
    const { start, last, lastHeight } = active;
    const isSamePlace = start.position[0] === last.position[0] && start.position[1] === last.position[1];
    if (isSamePlace && Math.abs(lastHeight - active.startHeight) < HEIGHT_TOLERANCE) return { ...last, z: start.z };
    if (!context.isEditorCameraActive) return { ...last, z: roundToHundredth(lastHeight) };
    const resting = restHeight(context.engine, active.objectIndex, last.position[0], last.position[1]);
    return { ...last, z: resting !== undefined && Math.abs(resting - lastHeight) <= HEIGHT_TOLERANCE ? null : roundToHundredth(lastHeight) };
  }

  function restoreStart(active: ObjectGesture): void {
    if (!active.hasStarted) return;
    sendPlacement(getContext().engine, active.objectIndex, active.channel, active.start, active.startHeight);
  }

  function pointerCancel(input: PointerInput): void {
    if (mountainController.pointerCancel(getContext(), input)) return;
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId) return;
    gesture = null;
    if (isObjectGesture(active)) restoreStart(active);
    if (isStrokeGesture(active)) undoStroke(active);
  }

  function wheel(input: WheelInput): boolean {
    const context = getContext();
    if (context.isInputLocked || !context.isEditorCameraActive) return false;
    const camera = context.cameraStore.current();
    if (camera === null) return false;
    context.cameraStore.update(context.engine, zoomCamera(camera.camera, wheelClicks(input.deltaY, input.deltaMode), camera.maxDistance));
    return true;
  }

  /** Shift смотрится в каждом кадре мазка: нажал посреди мазка — дальше «Поднять» опускает («Кисти рельефа», требование 11). */
  function noteShift(input: KeyInput): void {
    if (isStrokeGesture(gesture)) gesture.isLowering = input.shiftKey;
  }

  function keyUp(input: KeyInput): void {
    noteShift(input);
  }

  function keyDown(input: KeyInput): boolean {
    const context = getContext();
    if (context.isInputLocked) return false;
    noteShift(input);
    if (input.ctrlKey || input.altKey || input.metaKey) return false;
    if (input.code === "Escape") {
      if (mountainController.cancel(context)) return true;
      const active = gesture;
      if (isStrokeGesture(active)) {
        gesture = null;
        undoStroke(active);
        return true;
      }
      // Без жеста Esc снимает кисть или «Гору»: круг пропадает, щелчок снова выбирает, ручки — прежнего вида.
      if (active === null && (context.brush !== null || context.mountains.placing !== null)) {
        context.onHandleModeChange(context.handleMode);
        return true;
      }
      if (!isObjectGesture(active)) return false;
      gesture = null;
      restoreStart(active);
      return true;
    }
    const mode = MODE_KEYS[input.code];
    if (mode !== undefined && !input.shiftKey) {
      if (!context.areHandlesAvailable || isStrokeGesture(gesture) || mountainController.isActive()) return false;
      hovered = null;
      context.onHandleModeChange(mode);
      return true;
    }
    if (input.code === "KeyF" && !input.shiftKey) {
      if (!context.isEditorCameraActive) return false;
      if (context.selectedIndex !== null) focusCameraOnObject(context.cameraStore, context.engine, context.selectedIndex);
      return true;
    }
    return false;
  }

  /** Круг кисти на рельефе под указателем — «Кисти рельефа», требования 6–7: точки окружности садятся на землю и идут за её изгибом. */
  function drawBrush(context: SpaceSceneContext, context2d: CanvasRenderingContext2D, pixelRatio: number): void {
    const settings = isStrokeGesture(gesture) ? gesture.settings : context.brush;
    const pointer = isStrokeGesture(gesture) ? gesture.pointer : gesture === null ? hoverPoint : null;
    if (settings === null || pointer === null) return;
    const place = toVec3(context.engine.terrain_at(pointer[0], pointer[1]));
    if (place === undefined) return;
    const projection = projectionOf(context.engine);
    const ring = brushRingPoints([place[0], place[1]], settings.size / 2).map((point) => {
      const height = context.engine.terrain_height(point[0], point[1]);
      return typeof height === "number" ? projection.screenPoint(point[0], point[1], height) : undefined;
    });
    drawBrushCircle(context2d, ring, projection.screenPoint(place[0], place[1], place[2]), pixelRatio);
  }

  function draw(context2d: CanvasRenderingContext2D, pixelRatio: number): void {
    const context = getContext();
    drawBrush(context, context2d, pixelRatio);
    mountainController.draw(context, context2d, pixelRatio);
    if (context.selectedIndex === null) return;
    const rect = context.engine.object_rect(context.selectedIndex) as { corners?: Vec2[] } | undefined;
    if (rect?.corners !== undefined) drawSelectionQuad(context2d, rect.corners, context.selectedLabel, pixelRatio);
    if (!context.areHandlesAvailable || context.brush !== null) return;
    const subject = selectedHandleSubject(context);
    if (subject === null) return;
    const geometry = computeHandleGeometry(projectionOf(context.engine), subject.placement, context.handleMode, subject.baseHeight);
    if (geometry !== undefined) drawHandles(context2d, geometry, context.handleMode, hovered, pixelRatio);
  }

  return {
    pointerDown,
    pointerMove,
    pointerUp,
    pointerCancel,
    pointerLeave: () => {
      hovered = null;
      hoverPoint = null;
      mountainController.pointerLeave();
    },
    wheel,
    keyDown,
    keyUp,
    strokeFrame,
    draw,
    abandonGesture: () => {
      const active = gesture;
      gesture = null;
      if (isStrokeGesture(active)) active.onActiveChange(false);
      mountainController.abandon();
    },
  };
}
