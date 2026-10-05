import type { Engine } from "engine";
import type { MaskSet } from "./maskBytes";
import type { Vec2 } from "./objectPlacement";
import { applyPaintFrame, finishPaintStroke, orderedMasks, paintStrokeLayers, paintStrokeMasks, startPaintStroke, type PaintResult, type PaintStroke } from "./paintStroke";
import type { SceneSize } from "./sceneObjects";
import { brushPathPoints } from "./terrainBrush";
import type { TerrainCoverLayer } from "./terrainFile";
import { readTerrainSnapshot, toVec3 } from "./terrainReadings";

/** Вызовы движка, которыми пользуется покраска: точка на земле, рельеф проекта без файла и покрытия. */
export type PaintSceneEngine = Pick<Engine, "terrain_at" | "terrain_heights" | "set_terrain" | "set_covers">;

/** В группе «Материалы» выбран материал — кисть красит им: слои и маски файла рельефа, материал и числа кисти — «Покраска», требования 1–2. */
export type PaintContext = {
  material: string;
  size: number;
  strength: number;
  /** Слои покрытий файла рельефа; `null` — покрытий нет. */
  covers: readonly TerrainCoverLayer[] | null;
  masks: MaskSet;
  sceneSize: SceneSize;
  /** У проекта есть файл рельефа; без него первый мазок заводит его («Покраска», требование 15). */
  hasTerrainFile: boolean;
  /** Путь карты цвета `tint` файла рельефа: маска нового слоя его не занимает; карты нет — `null`. */
  tintPath: string | null;
  /** Отпускание после мазка: слои и изменившиеся маски — одно действие. */
  onCommit: (result: PaintResult) => void;
  /** Мазок брошен, а движок нечем вернуть к слоям файла (покрытий в нём не было) — страница собирает мир из показанных файлов заново. */
  onRestore: () => void;
};

export type PaintSceneContext = {
  engine: PaintSceneEngine;
  paint: PaintContext | null;
  /** Мазок начался или кончился любым способом — пока он идёт, изменения файлов снаружи ждут. */
  onStrokeActiveChange: (isActive: boolean) => void;
};

export type PaintPointer = { pointerId: number; button: number; buttons: number; x: number; y: number; shiftKey: boolean; timeStamp: number };

type PaintGesture = {
  pointerId: number;
  button: number;
  pointer: Vec2;
  isErasing: boolean;
  lastFrameMs: number;
  /** Точка кисти прошлого кадра; `null` — указатель ушёл с земли, путь до новой точки не тянется. */
  lastPoint: Vec2 | null;
  size: number;
  strength: number;
  stroke: PaintStroke;
  /** Слои и маски до мазка — Esc и сброс указателя возвращают их движку. */
  original: { covers: readonly TerrainCoverLayer[] | null; masks: MaskSet };
  hasChanged: boolean;
  /** Проект без файла рельефа: движок получит ровную землю лишь с первым кадром, что что-то меняет, — мазок без изменений его не трогает. */
  isFlatTerrainPending: boolean;
  onCommit: (result: PaintResult) => void;
  onRestore: () => void;
  onActiveChange: (isActive: boolean) => void;
};

export type PaintSceneController = {
  isActive: () => boolean;
  /** Нажатие при выбранной «Покрасить»; `true` — мазок начат. */
  start: (context: PaintSceneContext, input: PaintPointer) => boolean;
  /** Движение указателя; `true` — идёт мазок этого указателя. */
  pointerMove: (input: PaintPointer) => boolean;
  pointerUp: (input: PaintPointer) => boolean;
  pointerCancel: (context: PaintSceneContext, input: PaintPointer) => boolean;
  /** Shift смотрится в каждом кадре мазка: нажал посреди мазка — дальше кисть стирает. */
  noteShift: (isShift: boolean) => void;
  /** Esc: мазок, если он идёт, возвращает слои на место; `true` — мазок был. */
  cancel: (context: PaintSceneContext) => boolean;
  /** Кадр страницы: пока кнопка нажата, кисть красит, даже если указатель стоит. */
  frame: (context: PaintSceneContext, nowMs: number) => void;
  /** Мазок бросается без возврата слоёв: мир уже собран заново. */
  abandon: () => void;
  /** Круг кисти идущего мазка: поперечник и место указателя на экране. */
  circle: () => { size: number; pointer: Vec2 } | null;
};

const LEFT_BUTTON_BIT = 1;

/** Рельеф проекта без файла: движок получает ровную землю, чтобы принять покрытия («Покраска», требование 15). */
function prepareFlatTerrain(engine: PaintSceneEngine): boolean {
  const terrain = readTerrainSnapshot(engine.terrain_heights());
  return terrain !== undefined && engine.set_terrain(Float64Array.from(terrain.grid.heights), terrain.water, []) === undefined;
}

/** Покраска трёхмерной сцены: мазок кисти «Покрасить» по маскам покрытий с показом через `set_covers` в каждом кадре. */
export function createPaintSceneController(): PaintSceneController {
  let gesture: PaintGesture | null = null;

  function start(context: PaintSceneContext, input: PaintPointer): boolean {
    const paint = context.paint;
    if (paint === null) return false;
    const place = toVec3(context.engine.terrain_at(input.x, input.y));
    const stroke = startPaintStroke(paint.covers, paint.masks, paint.sceneSize, paint.material, paint.tintPath);
    if (place === undefined || stroke === null) return false;
    gesture = {
      pointerId: input.pointerId,
      button: input.button,
      pointer: [input.x, input.y],
      isErasing: input.shiftKey,
      lastFrameMs: input.timeStamp,
      lastPoint: [place[0], place[1]],
      size: paint.size,
      strength: paint.strength,
      stroke,
      original: { covers: paint.covers, masks: paint.masks },
      hasChanged: false,
      isFlatTerrainPending: !paint.hasTerrainFile,
      onCommit: paint.onCommit,
      onRestore: paint.onRestore,
      onActiveChange: context.onStrokeActiveChange,
    };
    context.onStrokeActiveChange(true);
    return true;
  }

  function frame(context: PaintSceneContext, nowMs: number): void {
    const active = gesture;
    if (active === null) return;
    const seconds = (nowMs - active.lastFrameMs) / 1000;
    active.lastFrameMs = nowMs;
    const place = toVec3(context.engine.terrain_at(active.pointer[0], active.pointer[1]));
    if (place === undefined) {
      active.lastPoint = null;
      return;
    }
    const point: Vec2 = [place[0], place[1]];
    const path = brushPathPoints(active.lastPoint, point, active.size);
    active.lastPoint = point;
    if (seconds <= 0) return;
    if (!applyPaintFrame(active.stroke, path, { size: active.size, strength: active.strength, seconds, isErasing: active.isErasing })) return;
    active.hasChanged = true;
    if (active.isFlatTerrainPending && prepareFlatTerrain(context.engine)) active.isFlatTerrainPending = false;
    if (!active.isFlatTerrainPending && context.engine.set_covers(paintStrokeLayers(active.stroke), paintStrokeMasks(active.stroke)) === undefined) return;
    // Движок отказал (игра пересобрана или пошла партия) — мазок бросается без действия.
    gesture = null;
    active.onActiveChange(false);
  }

  /** Слои и маски до мазка — Esc, сброс указателя: файлы не пишутся, действия нет («Покраска», требование 16). */
  function undoStroke(context: PaintSceneContext, active: PaintGesture): void {
    const { covers, masks } = active.original;
    if (active.hasChanged) {
      if (covers !== null && covers.length > 0) context.engine.set_covers(covers, orderedMasks(covers, masks));
      else active.onRestore();
    }
    active.onActiveChange(false);
  }

  function pointerUp(input: PaintPointer): boolean {
    const active = gesture;
    if (active === null || active.pointerId !== input.pointerId || (input.buttons & LEFT_BUTTON_BIT) !== 0) return false;
    gesture = null;
    const result = finishPaintStroke(active.stroke);
    if (result !== null) active.onCommit(result);
    active.onActiveChange(false);
    return true;
  }

  return {
    isActive: () => gesture !== null,
    start,
    pointerMove: (input) => {
      const active = gesture;
      if (active === null || active.pointerId !== input.pointerId) return false;
      active.pointer = [input.x, input.y];
      active.isErasing = input.shiftKey;
      return true;
    },
    pointerUp,
    pointerCancel: (context, input) => {
      const active = gesture;
      if (active === null || active.pointerId !== input.pointerId) return false;
      gesture = null;
      undoStroke(context, active);
      return true;
    },
    noteShift: (isShift) => {
      if (gesture !== null) gesture.isErasing = isShift;
    },
    cancel: (context) => {
      const active = gesture;
      if (active === null) return false;
      gesture = null;
      undoStroke(context, active);
      return true;
    },
    frame,
    abandon: () => {
      const active = gesture;
      gesture = null;
      active?.onActiveChange(false);
    },
    circle: () => (gesture === null ? null : { size: gesture.size, pointer: gesture.pointer }),
  };
}
