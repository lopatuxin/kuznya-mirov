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
  imprintHandleGeometry,
  imprintOutline,
  imprintToEntry,
  newImprintEntry,
  readImprint,
  scaledImprint,
  translatedImprint,
  turnedImprint,
  type Imprint,
  type StampShape,
} from "./imprintGeometry";
import type { Vec2 } from "./objectPlacement";
import type { PaintContext } from "./paintSceneController";
import { drawSelectionQuad } from "./selectionDrawing";
import type { BrushSettings } from "./terrainBrush";
import type { ImprintEntry } from "./terrainFile";
import { readTerrainSnapshot, toVec2, toVec3, type Vec3 } from "./terrainReadings";
import { createPlaneGrab, pointOnPlane, type PlaneGrab } from "./verticalGrab";

/** Вызовы движка, которыми пользуются отпечатки: выбор лучом, высота земли, живой показ через `set_terrain`. */
export type ImprintSceneEngine = Pick<Engine, "stamp_at" | "terrain_at" | "terrain_height" | "terrain_heights" | "set_terrain" | "screen_point">;

/** Кнопка «Отпечаток» выбрана: щелчок по земле ставит отпечаток этого штампа с этими числами. */
export type ImprintPlacement = { stamp: StampShape; width: number; height: number };

export type ImprintContext = {
  /** Отпечатки выбираются и правятся: трёхмерная сцена вне партии, паузы и повтора. */
  isEditable: boolean;
  /** Отпечатки файла рельефа как есть, в порядке файла — номер отпечатка тот же, что у `stamp_at`. */
  entries: readonly ImprintEntry[];
  selectedIndex: number | null;
  placing: ImprintPlacement | null;
  onSelect: (index: number) => void;
  /** Новый отпечаток: дописывается в конец файла, выбирается, инструмент возвращается к ручкам. */
  onPlace: (entry: ImprintEntry) => void;
  /** Отпускание после жеста: отпечаток целиком — одно действие. */
  onCommit: (index: number, entry: ImprintEntry) => void;
  /** Жест начался или кончился любым способом — пока он идёт, изменения файлов снаружи ждут. */
  onActiveChange: (isActive: boolean) => void;
};

/** Что отпечаток узнаёт у основного контроллера при каждом событии; контекст трёхмерной сцены подходит как есть. */
export type ImprintSceneContext = { engine: ImprintSceneEngine; handleMode: HandleMode; brush: BrushSettings | null; paint: PaintContext | null; imprints: ImprintContext };

type ImprintPointer = { pointerId: number; button: number; buttons: number; x: number; y: number; ctrlKey: boolean };

type ImprintGesture = {
  pointerId: number;
  index: number;
  /** Схваченная ручка; `null` — отпечаток взяли за тело. */
  hit: HandleHit | null;
  mode: HandleMode;
  start: Imprint;
  last: Imprint;
  /** Отпечатки файла на начало жеста: живой показ меняет один из них, Esc возвращает все. */
  entries: readonly ImprintEntry[];
  /** Высоты файла без отпечатков и вода на начало жеста — их `set_terrain` получает как есть. */
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

export type ImprintSceneController = {
  /** Нажатие при выбранном «Отпечатке»: ставит отпечаток там, где стоит пробный, а без него — под указателем. */
  place: (context: ImprintSceneContext, input: ImprintPointer) => void;
  /**
   * Кадр страницы: пока выбран «Отпечаток» и указатель над сценой (`pointer`), земля под ним уже несёт этот отпечаток —
   * видно, что получится, до щелчка. Указатель ушёл, инструмент сменился — земля возвращается к файлу. `view` — камера:
   * сменилась — под тем же указателем уже другое место.
   */
  previewFrame: (context: ImprintSceneContext, pointer: Vec2 | null, view: unknown) => void;
  /** Пробный отпечаток снимается сразу, а не в следующем кадре: перед запуском игры и повтора земля должна быть как в файле. */
  clearPreview: (context: ImprintSceneContext) => void;
  /** Нажатие на ручку выбранного отпечатка; `true` — жест начат. */
  startHandle: (context: ImprintSceneContext, input: ImprintPointer) => boolean;
  /** Нажатие мимо объектов: отпечаток под указателем выбирается и берётся за тело; `null` — под указателем отпечатков нет. */
  pickAt: (context: ImprintSceneContext, input: ImprintPointer) => boolean | null;
  /** Движение указателя; `true` — идёт жест отпечатка, остальное ему не нужно. */
  pointerMove: (context: ImprintSceneContext, input: ImprintPointer) => boolean;
  pointerUp: (context: ImprintSceneContext, input: ImprintPointer) => boolean;
  pointerCancel: (context: ImprintSceneContext, input: ImprintPointer) => boolean;
  /** Esc: жест, если он идёт, возвращает отпечаток на место; `true` — жест был. */
  cancel: (context: ImprintSceneContext) => boolean;
  /** Жест бросается без возврата отпечатка: мир уже собран заново. */
  abandon: () => void;
  isActive: () => boolean;
  pointerLeave: () => void;
  draw: (context: ImprintSceneContext, canvas: CanvasRenderingContext2D, pixelRatio: number) => void;
};

/**
 * Пробный отпечаток. Под каким указателем, с какими числами, камерой и отпечатками файла он поставлен — пока они те же,
 * его не переставляют. `entry` — сам отпечаток; `null` — под указателем нет земли или движок отпечаток не принял, и с
 * теми же условиями попытка не повторяется. `centerHeight` — высота земли в его середине сразу после показа: стала
 * другой — мир собран заново (отмена, правка файла снаружи), и отпечаток ставится снова.
 */
type ImprintPreview = { pointer: Vec2; key: string; view: unknown; entries: readonly ImprintEntry[]; entry: ImprintEntry | null; centerHeight: number };

/** Указатель на месте: щелчок там же, где был последний кадр пробного отпечатка. */
const SAME_POINTER_PX = 1;

function placementKey(placing: ImprintPlacement): string {
  return `${placing.stamp.name}|${placing.width}|${placing.height}`;
}

function isSamePointer(first: Vec2, second: Vec2): boolean {
  return Math.hypot(first[0] - second[0], first[1] - second[1]) < SAME_POINTER_PX;
}

/** «Редактор», требование 14: сдвиг указателя дальше 4 точек начинает перенос за тело. */
const BODY_DRAG_THRESHOLD_PX = 4;
const LEFT_BUTTON_BIT = 1;

function projectionOf(engine: ImprintSceneEngine): SpaceProjection {
  return { screenPoint: (x, y, z) => toVec2(engine.screen_point(x, y, z)) };
}

/** Высота видимой земли в месте сцены; движок, что её не отдал, — 0. */
function groundHeight(engine: ImprintSceneEngine, point: Vec2): number {
  const height = engine.terrain_height(point[0], point[1]);
  return typeof height === "number" ? height : 0;
}

function withImprint(entries: readonly ImprintEntry[], index: number, imprint: Imprint): ImprintEntry[] {
  return entries.map((entry, position) => (position === index ? imprintToEntry(imprint) : entry));
}

function translateAxisOf(hit: HandleHit | null): TranslateAxis {
  if (hit === "axis-x") return "x";
  if (hit === "axis-y") return "y";
  return "free";
}

/** Отпечатки трёхмерной сцены: выбор щелчком, ручки, перенос за тело, рамка по земле и живой показ через `set_terrain`. */
export function createImprintSceneController(): ImprintSceneController {
  let gesture: ImprintGesture | null = null;
  let hovered: HandleHit | null = null;
  let preview: ImprintPreview | null = null;

  function areHandlesShown(context: ImprintSceneContext): boolean {
    return context.imprints.isEditable && context.brush === null && context.paint === null && context.imprints.placing === null;
  }

  function selectedImprint(context: ImprintSceneContext): { index: number; imprint: Imprint } | null {
    const index = context.imprints.selectedIndex;
    const imprint = index === null ? null : readImprint(context.imprints.entries[index]);
    return index === null || imprint === null ? null : { index, imprint };
  }

  function handleGeometryOf(context: ImprintSceneContext, imprint: Imprint, mode: HandleMode): HandleGeometry | undefined {
    return imprintHandleGeometry(projectionOf(context.engine), imprint, mode, groundHeight(context.engine, imprint.position));
  }

  function place(context: ImprintSceneContext, input: ImprintPointer): void {
    const { placing } = context.imprints;
    if (placing === null) return;
    const shown = preview;
    // Пробный стоит под этим указателем с этими числами — он и остаётся; запись файла соберёт мир заново, возвращать землю не нужно.
    if (shown?.entry != null && shown.key === placementKey(placing) && isSamePointer(shown.pointer, [input.x, input.y])) {
      preview = null;
      context.imprints.onPlace(shown.entry);
      return;
    }
    // Пробный отстал от указателя или чисел: сначала земля без него, иначе луч упрётся в его склон.
    clearPreview(context);
    const point = toVec3(context.engine.terrain_at(input.x, input.y));
    if (point !== undefined) context.imprints.onPlace(newImprintEntry([point[0], point[1]], placing.stamp, placing.width, placing.height));
  }

  /** Пробный отпечаток снимается: земля снова как в файле — высоты и вода, что стоят сейчас, и отпечатки файла. */
  function clearPreview(context: ImprintSceneContext): void {
    const shown = preview;
    preview = null;
    if (shown?.entry == null) return;
    const snapshot = readTerrainSnapshot(context.engine.terrain_heights());
    if (snapshot !== undefined) context.engine.set_terrain(snapshot.grid.heights, snapshot.water, context.imprints.entries as ImprintEntry[]);
  }

  /** Тот же указатель, числа, камера и отпечатки файла, и мир не собран заново — пробный стоит где надо. */
  function isPreviewCurrent(context: ImprintSceneContext, pointer: Vec2, key: string, view: unknown): boolean {
    const shown = preview;
    if (shown === null || shown.key !== key || shown.view !== view || shown.entries !== context.imprints.entries || !isSamePointer(shown.pointer, pointer)) return false;
    return shown.entry === null || groundHeight(context.engine, shown.entry.position as Vec2) === shown.centerHeight;
  }

  function previewFrame(context: ImprintSceneContext, pointer: Vec2 | null, view: unknown): void {
    const placing = context.imprints.isEditable && gesture === null ? context.imprints.placing : null;
    if (placing === null || pointer === null) {
      clearPreview(context);
      return;
    }
    const key = placementKey(placing);
    if (isPreviewCurrent(context, pointer, key, view)) return;
    const { engine } = context;
    const entries = context.imprints.entries;
    const snapshot = readTerrainSnapshot(engine.terrain_heights());
    if (snapshot === undefined) return;
    // Место ищется по земле без пробного: иначе луч упирался бы в его склон, и холм полз бы к камере. Высоты файла и вода
    // от пробного не меняются — один снимок годится и чтобы снять его, и чтобы поставить новый.
    if (preview?.entry != null) engine.set_terrain(snapshot.grid.heights, snapshot.water, entries as ImprintEntry[]);
    const ground = toVec3(engine.terrain_at(pointer[0], pointer[1]));
    const entry = ground === undefined ? null : newImprintEntry([ground[0], ground[1]], placing.stamp, placing.width, placing.height);
    const isShown = entry !== null && engine.set_terrain(snapshot.grid.heights, snapshot.water, [...entries, entry]) === undefined;
    const shownEntry = isShown ? entry : null;
    preview = { pointer, key, view, entries, entry: shownEntry, centerHeight: shownEntry === null ? 0 : groundHeight(engine, shownEntry.position as Vec2) };
  }

  function begin(context: ImprintSceneContext, input: ImprintPointer, index: number, imprint: Imprint, hit: HandleHit | null, geometry: HandleGeometry | null): boolean {
    const { engine } = context;
    const snapshot = readTerrainSnapshot(engine.terrain_heights());
    if (snapshot === undefined) return false;
    const point: Vec2 = [input.x, input.y];
    const mode: HandleMode = hit === null ? "translate" : context.handleMode;
    let plane: PlaneGrab | null = null;
    let planeHeight = 0;
    let planeStart: Vec2 | null = null;
    if (mode !== "scale") {
      // Плоскость стоит на высоте схваченной точки: за тело — земля под указателем, за ручку — середина отпечатка.
      // Земля под указателем, меняясь при переносе, плоскость не сдвигает.
      const baseHeight = groundHeight(engine, imprint.position);
      const under = toVec3(engine.terrain_at(input.x, input.y));
      const anchor: Vec3 = under ?? [imprint.position[0], imprint.position[1], baseHeight];
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
      start: imprint,
      last: imprint,
      entries: context.imprints.entries,
      heights: snapshot.grid.heights,
      water: snapshot.water,
      startPoint: point,
      hasStarted: hit !== null,
      plane,
      planeHeight,
      planeStart,
      geometry,
      onActiveChange: context.imprints.onActiveChange,
    };
    context.imprints.onActiveChange(true);
    return true;
  }

  function startHandle(context: ImprintSceneContext, input: ImprintPointer): boolean {
    const subject = areHandlesShown(context) ? selectedImprint(context) : null;
    if (subject === null) return false;
    const geometry = handleGeometryOf(context, subject.imprint, context.handleMode);
    const hit = geometry === undefined ? null : hitTestHandles(geometry, context.handleMode, [input.x, input.y]);
    if (geometry === undefined || hit === null) return false;
    return begin(context, input, subject.index, subject.imprint, hit, geometry);
  }

  function pickAt(context: ImprintSceneContext, input: ImprintPointer): boolean | null {
    if (!context.imprints.isEditable) return null;
    const picked = context.engine.stamp_at(input.x, input.y) as number | undefined;
    if (typeof picked !== "number") return null;
    context.imprints.onSelect(picked);
    const imprint = readImprint(context.imprints.entries[picked]);
    return imprint !== null && begin(context, input, picked, imprint, null, null);
  }

  /** Новые числа отпечатка по жесту; `null` — место под указателем не найдено (луч не пересекает плоскость): отпечаток стоит, пока луч не вернётся. */
  function nextImprint(active: ImprintGesture, point: Vec2, ctrlKey: boolean): Imprint | null {
    if (active.mode === "scale") {
      const geometry = active.geometry;
      if (geometry === null || active.hit === null) return null;
      const axis = scaleAxisOfHit(active.hit) as ScaleAxis;
      const arrow = arrowVectorOfHit(geometry, active.hit);
      const factor =
        axis === "uniform" || arrow === null
          ? scaleFactorUniform(active.startPoint, point, ARROW_LENGTH_PX)
          : scaleFactorAlong(geometry.center, arrow, active.startPoint, point);
      return scaledImprint(active.start, axis, factor, ctrlKey);
    }
    const now = active.plane === null ? undefined : pointOnPlane(active.plane, point, active.planeHeight);
    if (now === undefined || active.planeStart === null) return null;
    if (active.mode === "rotate") return turnedImprint(active.start, active.planeStart, now, ctrlKey);
    const delta: Vec2 = [now[0] - active.planeStart[0], now[1] - active.planeStart[1]];
    return translatedImprint(active.start, delta, translateAxisOf(active.hit), ctrlKey);
  }

  function updateHover(context: ImprintSceneContext, input: ImprintPointer): void {
    hovered = null;
    const subject = areHandlesShown(context) ? selectedImprint(context) : null;
    if (subject === null) return;
    const geometry = handleGeometryOf(context, subject.imprint, context.handleMode);
    if (geometry !== undefined) hovered = hitTestHandles(geometry, context.handleMode, [input.x, input.y]);
  }

  function pointerMove(context: ImprintSceneContext, input: ImprintPointer): boolean {
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
    const next = nextImprint(active, point, input.ctrlKey);
    if (next === null) return true;
    active.last = next;
    // Живой показ: земля, объекты без `z` и свет обновляются на глазах; отказ движка бросает жест.
    if (context.engine.set_terrain(active.heights, active.water, withImprint(active.entries, active.index, next)) !== undefined) {
      gesture = null;
      active.onActiveChange(false);
    }
    return true;
  }

  function restore(context: ImprintSceneContext, active: ImprintGesture): void {
    if (active.hasStarted) context.engine.set_terrain(active.heights, active.water, active.entries as ImprintEntry[]);
    active.onActiveChange(false);
  }

  function pointerUp(context: ImprintSceneContext, input: ImprintPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId || (input.buttons & LEFT_BUTTON_BIT) !== 0) return true;
    gesture = null;
    const entry = imprintToEntry(active.last);
    if (active.hasStarted && JSON.stringify(entry) !== JSON.stringify(imprintToEntry(active.start))) context.imprints.onCommit(active.index, entry);
    active.onActiveChange(false);
    return true;
  }

  function pointerCancel(context: ImprintSceneContext, input: ImprintPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId) return true;
    gesture = null;
    restore(context, active);
    return true;
  }

  function cancel(context: ImprintSceneContext): boolean {
    const active = gesture;
    if (active === null) return false;
    gesture = null;
    restore(context, active);
    return true;
  }

  function abandon(): void {
    const active = gesture;
    gesture = null;
    preview = null;
    active?.onActiveChange(false);
  }

  /** Рамка отпечатка — стороны прямоугольника по видимой земле, цвет и толщина как у рамки объекта. */
  function drawOutline(context: ImprintSceneContext, canvas: CanvasRenderingContext2D, imprint: Imprint, label: string, pixelRatio: number): void {
    const projection = projectionOf(context.engine);
    const outline = imprintOutline(imprint)
      .map((point) => projection.screenPoint(point[0], point[1], groundHeight(context.engine, point)))
      .filter((point): point is Vec2 => point !== undefined);
    if (outline.length >= 3) drawSelectionQuad(canvas, outline, label, pixelRatio);
  }

  /** Рамка выбранного или пробного отпечатка; ручки — в середине на земле. */
  function draw(context: ImprintSceneContext, canvas: CanvasRenderingContext2D, pixelRatio: number): void {
    if (!context.imprints.isEditable) return;
    const shown = preview?.entry == null ? null : readImprint(preview.entry);
    if (shown !== null) drawOutline(context, canvas, shown, shown.stamp, pixelRatio);
    const selected = selectedImprint(context);
    const subject = gesture === null ? selected : { index: gesture.index, imprint: gesture.last };
    if (subject === null) return;
    drawOutline(context, canvas, subject.imprint, `Отпечаток ${subject.index + 1}`, pixelRatio);
    if (!areHandlesShown(context)) return;
    const geometry = handleGeometryOf(context, subject.imprint, context.handleMode);
    if (geometry !== undefined) drawHandles(canvas, geometry, context.handleMode, hovered, pixelRatio);
  }

  return {
    place,
    previewFrame,
    clearPreview,
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
