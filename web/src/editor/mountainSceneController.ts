import type { Engine } from "engine";
import { drawHandles } from "./handleDrawing";
import {
  ARROW_LENGTH_PX,
  arrowVectorOfHit,
  hitTestHandles,
  scaleAxisOfHit,
  type HandleGeometry,
  type HandleHit,
  type HandleMode,
  type SpaceProjection,
} from "./handleGeometry";
import { scaleFactorAlong, scaleFactorUniform, type ScaleAxis, type TranslateAxis } from "./handleMath";
import {
  mountainHandleGeometry,
  mountainOutline,
  mountainToEntry,
  newMountainEntry,
  readMountain,
  scaledMountain,
  translatedMountain,
  turnedMountain,
  type Mountain,
  type StampShape,
} from "./mountainGeometry";
import type { Vec2 } from "./objectPlacement";
import { drawSelectionQuad } from "./selectionDrawing";
import type { BrushSettings } from "./terrainBrush";
import type { MountainEntry } from "./terrainFile";
import { readTerrainSnapshot, toVec2, toVec3, type Vec3 } from "./terrainReadings";
import { createPlaneGrab, pointOnPlane, type PlaneGrab } from "./verticalGrab";

/** Вызовы движка, которыми пользуются горы: выбор лучом, высота земли, живой показ через `set_terrain`. */
export type MountainSceneEngine = Pick<Engine, "stamp_at" | "terrain_at" | "terrain_height" | "terrain_heights" | "set_terrain" | "screen_point">;

/** Кнопка «Гора» выбрана: щелчок по земле ставит гору этого штампа с этими числами. */
export type MountainPlacement = { stamp: StampShape; width: number; height: number };

export type MountainContext = {
  /** Горы выбираются и правятся: трёхмерная сцена вне партии, паузы и повтора. */
  isEditable: boolean;
  /** Горы файла рельефа как есть, в порядке файла — номер горы тот же, что у `stamp_at`. */
  entries: readonly MountainEntry[];
  selectedIndex: number | null;
  placing: MountainPlacement | null;
  onSelect: (index: number) => void;
  /** Новая гора: дописывается в конец файла, выбирается, инструмент возвращается к ручкам. */
  onPlace: (entry: MountainEntry) => void;
  /** Отпускание после жеста: гора целиком — одно действие. */
  onCommit: (index: number, entry: MountainEntry) => void;
  /** Жест начался или кончился любым способом — пока он идёт, изменения файлов снаружи ждут. */
  onActiveChange: (isActive: boolean) => void;
};

/** Что гора узнаёт у основного контроллера при каждом событии; контекст трёхмерной сцены подходит как есть. */
export type MountainSceneContext = { engine: MountainSceneEngine; handleMode: HandleMode; brush: BrushSettings | null; mountains: MountainContext };

type MountainPointer = { pointerId: number; button: number; buttons: number; x: number; y: number; ctrlKey: boolean };

type MountainGesture = {
  pointerId: number;
  index: number;
  /** Схваченная ручка; `null` — гору взяли за тело. */
  hit: HandleHit | null;
  mode: HandleMode;
  start: Mountain;
  last: Mountain;
  /** Горы файла на начало жеста: живой показ меняет одну из них, Esc возвращает все. */
  entries: readonly MountainEntry[];
  /** Высоты файла без гор и вода на начало жеста — их `set_terrain` получает как есть. */
  heights: Float64Array;
  water: unknown;
  startPoint: Vec2;
  hasStarted: boolean;
  /** Горизонтальная плоскость переноса и поворота и место на ней под указателем в начале жеста. */
  plane: PlaneGrab | null;
  planeHeight: number;
  planeStart: Vec2 | null;
  /** Ручки на начало жеста — по ним считается доля масштаба. */
  geometry: HandleGeometry | null;
  onActiveChange: (isActive: boolean) => void;
};

export type MountainSceneController = {
  /** Нажатие при выбранной «Горе»: ставит гору под указателем. */
  place: (context: MountainSceneContext, input: MountainPointer) => void;
  /** Нажатие на ручку выбранной горы; `true` — жест начат. */
  startHandle: (context: MountainSceneContext, input: MountainPointer) => boolean;
  /** Нажатие мимо объектов: гора под указателем выбирается и берётся за тело; `null` — под указателем гор нет. */
  pickAt: (context: MountainSceneContext, input: MountainPointer) => boolean | null;
  /** Движение указателя; `true` — идёт жест горы, остальное ему не нужно. */
  pointerMove: (context: MountainSceneContext, input: MountainPointer) => boolean;
  pointerUp: (context: MountainSceneContext, input: MountainPointer) => boolean;
  pointerCancel: (context: MountainSceneContext, input: MountainPointer) => boolean;
  /** Esc: жест, если он идёт, возвращает гору на место; `true` — жест был. */
  cancel: (context: MountainSceneContext) => boolean;
  /** Жест бросается без возврата горы: мир уже собран заново. */
  abandon: () => void;
  isActive: () => boolean;
  pointerLeave: () => void;
  draw: (context: MountainSceneContext, canvas: CanvasRenderingContext2D, pixelRatio: number) => void;
};

/** «Редактор», требование 14: сдвиг указателя дальше 4 точек начинает перенос за тело. */
const BODY_DRAG_THRESHOLD_PX = 4;
const LEFT_BUTTON_BIT = 1;

function projectionOf(engine: MountainSceneEngine): SpaceProjection {
  return { screenPoint: (x, y, z) => toVec2(engine.screen_point(x, y, z)) };
}

/** Высота видимой земли в месте сцены; движок, что её не отдал, — 0. */
function groundHeight(engine: MountainSceneEngine, point: Vec2): number {
  const height = engine.terrain_height(point[0], point[1]);
  return typeof height === "number" ? height : 0;
}

function withMountain(entries: readonly MountainEntry[], index: number, mountain: Mountain): MountainEntry[] {
  return entries.map((entry, position) => (position === index ? mountainToEntry(mountain) : entry));
}

function translateAxisOf(hit: HandleHit | null): TranslateAxis {
  if (hit === "axis-x") return "x";
  if (hit === "axis-y") return "y";
  return "free";
}

/** Горы трёхмерной сцены: выбор щелчком, ручки, перенос за тело, рамка по земле и живой показ через `set_terrain`. */
export function createMountainSceneController(): MountainSceneController {
  let gesture: MountainGesture | null = null;
  let hovered: HandleHit | null = null;

  function areHandlesShown(context: MountainSceneContext): boolean {
    return context.mountains.isEditable && context.brush === null && context.mountains.placing === null;
  }

  function selectedMountain(context: MountainSceneContext): { index: number; mountain: Mountain } | null {
    const index = context.mountains.selectedIndex;
    const mountain = index === null ? null : readMountain(context.mountains.entries[index]);
    return index === null || mountain === null ? null : { index, mountain };
  }

  function handleGeometryOf(context: MountainSceneContext, mountain: Mountain, mode: HandleMode): HandleGeometry | undefined {
    return mountainHandleGeometry(projectionOf(context.engine), mountain, mode, groundHeight(context.engine, mountain.position));
  }

  function place(context: MountainSceneContext, input: MountainPointer): void {
    const { placing } = context.mountains;
    const point = toVec3(context.engine.terrain_at(input.x, input.y));
    if (placing === null || point === undefined) return;
    context.mountains.onPlace(newMountainEntry([point[0], point[1]], placing.stamp, placing.width, placing.height));
  }

  function begin(context: MountainSceneContext, input: MountainPointer, index: number, mountain: Mountain, hit: HandleHit | null, geometry: HandleGeometry | null): boolean {
    const { engine } = context;
    const snapshot = readTerrainSnapshot(engine.terrain_heights());
    if (snapshot === undefined) return false;
    const point: Vec2 = [input.x, input.y];
    const mode: HandleMode = hit === null ? "translate" : context.handleMode;
    let plane: PlaneGrab | null = null;
    let planeHeight = 0;
    let planeStart: Vec2 | null = null;
    if (mode !== "scale") {
      // Плоскость стоит на высоте схваченной точки: за тело — земля под указателем, за ручку — середина горы.
      // Земля под указателем, меняясь при переносе, плоскость не сдвигает.
      const baseHeight = groundHeight(engine, mountain.position);
      const under = toVec3(engine.terrain_at(input.x, input.y));
      const anchor: Vec3 = under ?? [mountain.position[0], mountain.position[1], baseHeight];
      planeHeight = hit === null ? anchor[2] : baseHeight;
      plane = createPlaneGrab(projectionOf(engine), anchor) ?? null;
      planeStart = plane === null ? null : (pointOnPlane(plane, point, planeHeight) ?? null);
      if (plane === null || planeStart === null) return false;
    }
    gesture = {
      pointerId: input.pointerId,
      index,
      hit,
      mode,
      start: mountain,
      last: mountain,
      entries: context.mountains.entries,
      heights: snapshot.grid.heights,
      water: snapshot.water,
      startPoint: point,
      hasStarted: hit !== null,
      plane,
      planeHeight,
      planeStart,
      geometry,
      onActiveChange: context.mountains.onActiveChange,
    };
    context.mountains.onActiveChange(true);
    return true;
  }

  function startHandle(context: MountainSceneContext, input: MountainPointer): boolean {
    const subject = areHandlesShown(context) ? selectedMountain(context) : null;
    if (subject === null) return false;
    const geometry = handleGeometryOf(context, subject.mountain, context.handleMode);
    const hit = geometry === undefined ? null : hitTestHandles(geometry, context.handleMode, [input.x, input.y]);
    if (geometry === undefined || hit === null) return false;
    return begin(context, input, subject.index, subject.mountain, hit, geometry);
  }

  function pickAt(context: MountainSceneContext, input: MountainPointer): boolean | null {
    if (!context.mountains.isEditable) return null;
    const picked = context.engine.stamp_at(input.x, input.y) as number | undefined;
    if (typeof picked !== "number") return null;
    context.mountains.onSelect(picked);
    const mountain = readMountain(context.mountains.entries[picked]);
    return mountain !== null && begin(context, input, picked, mountain, null, null);
  }

  /** Новые числа горы по жесту; `null` — место под указателем не найдено (луч не пересекает плоскость): гора стоит, пока луч не вернётся. */
  function nextMountain(active: MountainGesture, point: Vec2, ctrlKey: boolean): Mountain | null {
    if (active.mode === "scale") {
      const geometry = active.geometry;
      if (geometry === null || active.hit === null) return null;
      const axis = scaleAxisOfHit(active.hit) as ScaleAxis;
      const arrow = arrowVectorOfHit(geometry, active.hit);
      const factor =
        axis === "uniform" || arrow === null
          ? scaleFactorUniform(active.startPoint, point, ARROW_LENGTH_PX)
          : scaleFactorAlong(geometry.center, arrow, active.startPoint, point);
      return scaledMountain(active.start, axis, factor, ctrlKey);
    }
    const now = active.plane === null ? undefined : pointOnPlane(active.plane, point, active.planeHeight);
    if (now === undefined || active.planeStart === null) return null;
    if (active.mode === "rotate") return turnedMountain(active.start, active.planeStart, now, ctrlKey);
    const delta: Vec2 = [now[0] - active.planeStart[0], now[1] - active.planeStart[1]];
    return translatedMountain(active.start, delta, translateAxisOf(active.hit), ctrlKey);
  }

  function updateHover(context: MountainSceneContext, input: MountainPointer): void {
    hovered = null;
    const subject = areHandlesShown(context) ? selectedMountain(context) : null;
    if (subject === null) return;
    const geometry = handleGeometryOf(context, subject.mountain, context.handleMode);
    if (geometry !== undefined) hovered = hitTestHandles(geometry, context.handleMode, [input.x, input.y]);
  }

  function pointerMove(context: MountainSceneContext, input: MountainPointer): boolean {
    const active = gesture;
    if (active === null) {
      updateHover(context, input);
      return false;
    }
    if (active.pointerId !== input.pointerId) return true;
    const point: Vec2 = [input.x, input.y];
    if (!active.hasStarted) {
      if (Math.hypot(point[0] - active.startPoint[0], point[1] - active.startPoint[1]) <= BODY_DRAG_THRESHOLD_PX) return true;
      active.hasStarted = true;
    }
    const next = nextMountain(active, point, input.ctrlKey);
    if (next === null) return true;
    active.last = next;
    // Живой показ: земля, объекты без `z` и свет обновляются на глазах; отказ движка бросает жест.
    if (context.engine.set_terrain(active.heights, active.water, withMountain(active.entries, active.index, next)) !== undefined) {
      gesture = null;
      active.onActiveChange(false);
    }
    return true;
  }

  function restore(context: MountainSceneContext, active: MountainGesture): void {
    if (active.hasStarted) context.engine.set_terrain(active.heights, active.water, active.entries as MountainEntry[]);
    active.onActiveChange(false);
  }

  function pointerUp(context: MountainSceneContext, input: MountainPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId || (input.buttons & LEFT_BUTTON_BIT) !== 0) return true;
    gesture = null;
    const entry = mountainToEntry(active.last);
    if (active.hasStarted && JSON.stringify(entry) !== JSON.stringify(mountainToEntry(active.start))) context.mountains.onCommit(active.index, entry);
    active.onActiveChange(false);
    return true;
  }

  function pointerCancel(context: MountainSceneContext, input: MountainPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId) return true;
    gesture = null;
    restore(context, active);
    return true;
  }

  function cancel(context: MountainSceneContext): boolean {
    const active = gesture;
    if (active === null) return false;
    gesture = null;
    restore(context, active);
    return true;
  }

  function abandon(): void {
    const active = gesture;
    gesture = null;
    active?.onActiveChange(false);
  }

  /** Рамка — стороны прямоугольника по видимой земле, цвет и толщина как у рамки объекта; ручки — в середине на земле. */
  function draw(context: MountainSceneContext, canvas: CanvasRenderingContext2D, pixelRatio: number): void {
    if (!context.mountains.isEditable) return;
    const selected = selectedMountain(context);
    const subject = gesture === null ? selected : { index: gesture.index, mountain: gesture.last };
    if (subject === null) return;
    const projection = projectionOf(context.engine);
    const outline = mountainOutline(subject.mountain)
      .map((point) => projection.screenPoint(point[0], point[1], groundHeight(context.engine, point)))
      .filter((point): point is Vec2 => point !== undefined);
    if (outline.length >= 3) drawSelectionQuad(canvas, outline, `Гора ${subject.index + 1}`, pixelRatio);
    if (!areHandlesShown(context)) return;
    const geometry = handleGeometryOf(context, subject.mountain, context.handleMode);
    if (geometry !== undefined) drawHandles(canvas, geometry, context.handleMode, hovered, pixelRatio);
  }

  return {
    place,
    startHandle,
    pickAt,
    pointerMove,
    pointerUp,
    pointerCancel,
    cancel,
    abandon,
    isActive: () => gesture !== null,
    pointerLeave: () => {
      hovered = null;
    },
    draw,
  };
}
