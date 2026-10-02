import { areMasksEqual, type MaskBytes, type MaskSet } from "./maskBytes";
import type { SceneSize } from "./sceneObjects";
import type { Vec2 } from "./objectPlacement";
import { MAX_COVER_LAYERS, newMaskPath, topLayerIndex } from "./paintLayers";
import { brushWeight, MAX_FRAME_SECONDS } from "./terrainBrush";
import type { TerrainCoverLayer } from "./terrainFile";

/** Маска нового слоя — четыре точки на клетку сцены («Покраска», требование 8). */
const MASK_POINTS_PER_CELL = 4;
const STRENGTH_DIVISOR = 10;
const BYTE_MAX = 255;

/** Маска в работе: байты — копия, дробные значения — то, что мазок хранит между кадрами («Покраска», требование 11). */
type WorkingMask = { width: number; height: number; bytes: Uint8Array; fractions: Float64Array };

export type PaintStroke = {
  material: string;
  sceneSize: SceneSize;
  /** Путь карты цвета файла рельефа: маска нового слоя его не занимает. */
  tintPath: string | null;
  layers: TerrainCoverLayer[];
  isLayersChanged: boolean;
  baseMasks: MaskSet;
  working: Map<string, WorkingMask>;
};

/** Что кисть красит за кадр: поперечник, сила, секунды кадра и стирает ли она (Shift). */
export type PaintFrame = { size: number; strength: number; seconds: number; isErasing: boolean };

/** Итог мазка: слои покрытий и только те маски, байты которых изменились или которых не было. */
export type PaintResult = { covers: TerrainCoverLayer[]; masks: MaskSet };

/**
 * Начало мазка материалом по слоям файла и их маскам. У слоя с маской, которой нет в `masks`, мазка нет —
 * `null`: красить без байтов нечем.
 */
export function startPaintStroke(covers: readonly TerrainCoverLayer[] | null, masks: MaskSet, sceneSize: SceneSize, material: string, tintPath: string | null): PaintStroke | null {
  const layers = covers === null ? [] : covers.map((layer) => ({ ...layer }));
  if (layers.some((layer) => layer.mask !== undefined && masks[layer.mask] === undefined)) return null;
  return { material, sceneSize, tintPath, layers, isLayersChanged: false, baseMasks: masks, working: new Map() };
}

function workingMask(stroke: PaintStroke, path: string): WorkingMask {
  const existing = stroke.working.get(path);
  if (existing !== undefined) return existing;
  const base = stroke.baseMasks[path] as MaskBytes;
  const created = { width: base.width, height: base.height, bytes: Uint8Array.from(base.pixels), fractions: Float64Array.from(base.pixels, (value) => value / BYTE_MAX) };
  stroke.working.set(path, created);
  return created;
}

function addBlackMask(stroke: PaintStroke, path: string): void {
  const width = MASK_POINTS_PER_CELL * stroke.sceneSize.width;
  const height = MASK_POINTS_PER_CELL * stroke.sceneSize.height;
  stroke.working.set(path, { width, height, bytes: new Uint8Array(width * height), fractions: new Float64Array(width * height) });
}

/**
 * Слой материала на мазок — «Покраска», требования 8–9: без Shift первый кадр кладёт материал слоем, а слою без
 * маски даёт чёрную; с Shift слой должен уже быть выше первого и с маской. `null` — мазок ничего не делает.
 */
function targetLayerIndex(stroke: PaintStroke, isErasing: boolean): number | null {
  const { layers, material, tintPath } = stroke;
  let index = topLayerIndex(layers, material);
  if (isErasing) return index > 0 && layers[index]?.mask !== undefined ? index : null;
  if (index < 0) {
    if (layers.length >= MAX_COVER_LAYERS) return null;
    index = layers.length;
    const mask = index === 0 ? undefined : newMaskPath(layers, material, tintPath);
    layers.push(mask === undefined ? { material } : { material, mask });
    if (mask !== undefined) addBlackMask(stroke, mask);
    stroke.isLayersChanged = true;
  } else if (index > 0 && layers[index]?.mask === undefined) {
    const mask = newMaskPath(layers, material, tintPath);
    layers[index] = { ...(layers[index] as TerrainCoverLayer), mask };
    addBlackMask(stroke, mask);
    stroke.isLayersChanged = true;
  }
  return index;
}

/** Точки маски в кругу кисти двигаются к 1 (`isRaising`) или к 0 по `m ← 1 − (1 − m) × e^(−k × w × dt)`, `m ← m × e^(−k × w × dt)` — требование 7. */
function moveMask(stroke: PaintStroke, mask: WorkingMask, center: Vec2, radius: number, rate: number, isRaising: boolean): void {
  const { width: sceneWidth, height: sceneHeight } = stroke.sceneSize;
  const { width, height, bytes, fractions } = mask;
  const cellWidth = sceneWidth / width;
  const cellHeight = sceneHeight / height;
  const firstColumn = Math.max(0, Math.ceil((center[0] - radius) / cellWidth - 0.5));
  const lastColumn = Math.min(width - 1, Math.floor((center[0] + radius) / cellWidth - 0.5));
  const firstRow = Math.max(0, Math.ceil((center[1] - radius) / cellHeight - 0.5));
  const lastRow = Math.min(height - 1, Math.floor((center[1] + radius) / cellHeight - 0.5));
  for (let row = firstRow; row <= lastRow; row += 1) {
    for (let column = firstColumn; column <= lastColumn; column += 1) {
      const weight = brushWeight(Math.hypot((column + 0.5) * cellWidth - center[0], (row + 0.5) * cellHeight - center[1]), radius);
      if (weight === 0) continue;
      const index = row * width + column;
      const fade = Math.exp(-rate * weight);
      const next = isRaising ? 1 - (1 - (fractions[index] as number)) * fade : (fractions[index] as number) * fade;
      fractions[index] = next;
      bytes[index] = Math.round(next * BYTE_MAX);
    }
  }
}

/**
 * Кадр покраски — «Покраска», требования 6–10: без Shift кисть поднимает маску слоя материала и опускает маски
 * слоёв над ним; с Shift опускает маску слоя материала. Секунды кадра делятся поровну между точками пути, как у
 * кистей рельефа. `false` — кадр ничего не изменил и движку нечего слать.
 */
export function applyPaintFrame(stroke: PaintStroke, path: readonly Vec2[], frame: PaintFrame): boolean {
  const target = targetLayerIndex(stroke, frame.isErasing);
  if (target === null) return false;
  const seconds = Math.min(Math.max(frame.seconds, 0), MAX_FRAME_SECONDS) / path.length;
  const rate = (frame.strength / STRENGTH_DIVISOR) * seconds;
  const radius = frame.size / 2;
  const moved = stroke.layers.flatMap((layer, index) => {
    if (layer.mask === undefined || index < target || (frame.isErasing && index > target)) return [];
    return [{ mask: workingMask(stroke, layer.mask), isRaising: index === target && !frame.isErasing }];
  });
  for (const point of path) {
    for (const { mask, isRaising } of moved) moveMask(stroke, mask, point, radius, rate, isRaising);
  }
  return true;
}

/** Слои мазка в виде файла рельефа — их `set_covers` получает каждый кадр. */
export function paintStrokeLayers(stroke: PaintStroke): TerrainCoverLayer[] {
  return stroke.layers.map((layer) => ({ ...layer }));
}

/** Маски слоёв по одной на слой с маской, по порядку слоёв, как их ждёт `set_covers`. */
export function orderedMasks(layers: readonly TerrainCoverLayer[], masks: MaskSet): MaskBytes[] {
  return layers.flatMap((layer) => (layer.mask === undefined ? [] : [masks[layer.mask] as MaskBytes]));
}

/** Маски мазка в том виде, как их ждёт `set_covers`: байты в работе поверх масок файла. */
export function paintStrokeMasks(stroke: PaintStroke): MaskBytes[] {
  const current: Record<string, MaskBytes> = { ...stroke.baseMasks };
  for (const [path, mask] of stroke.working) current[path] = { width: mask.width, height: mask.height, pixels: mask.bytes };
  return orderedMasks(stroke.layers, current);
}

/** Итог мазка при отпускании — требование 12; `null` — ни слои, ни байты масок не изменились, мазок не действие. */
export function finishPaintStroke(stroke: PaintStroke): PaintResult | null {
  const masks: Record<string, MaskBytes> = {};
  for (const [path, mask] of stroke.working) {
    const result: MaskBytes = { width: mask.width, height: mask.height, pixels: mask.bytes };
    const base = stroke.baseMasks[path];
    if (base === undefined || !areMasksEqual(base, result)) masks[path] = result;
  }
  if (!stroke.isLayersChanged && Object.keys(masks).length === 0) return null;
  return { covers: paintStrokeLayers(stroke), masks };
}
