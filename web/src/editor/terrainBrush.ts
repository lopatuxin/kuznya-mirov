import type { Vec2 } from "./objectPlacement";
import type { TerrainGrid } from "./terrainFile";

export type BrushKind = "raise" | "level" | "smooth";

/** Размер и сила кисти — числа полей над сценой: пределы и значения при открытии редактора («Кисти рельефа», требование 2). */
export type BrushLimits = { min: number; max: number; initial: number };
export const BRUSH_SIZE_LIMITS: BrushLimits = { min: 1, max: 64, initial: 4 };
export const BRUSH_STRENGTH_LIMITS: BrushLimits = { min: 1, max: 100, initial: 50 };

/** Что кисть знает о себе, пока рисует: вид, поперечник круга в клетках и сила. */
export type BrushSettings = { kind: BrushKind; size: number; strength: number };

/** Кадр мазка дольше этого считается за него: вкладка, что стояла в фоне, не даёт одного огромного шага. */
export const MAX_FRAME_SECONDS = 0.1;

const RAISE_STRENGTH_DIVISOR = 50;
const LEVEL_STRENGTH_DIVISOR = 10;
/** Кисть идёт от кадра к кадру шагами не длиннее четверти размера — след сплошной. */
const PATH_STEP_SIZE_FRACTION = 0.25;

/** Число, набранное в поле: запятая как точка; пусто и не число — `null`, поле вернёт прежнее значение. */
export function parseNumberField(text: string): number | null {
  const trimmed = text.trim().replace(",", ".");
  if (trimmed === "") return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

export function clampToLimits(value: number, limits: BrushLimits): number {
  return Math.min(limits.max, Math.max(limits.min, value));
}

/** Вес точки высот на расстоянии `distance` по плоскости от точки кисти радиуса `radius` — «Кисти рельефа», требование 10. */
export function brushWeight(distance: number, radius: number): number {
  if (distance >= radius) return 0;
  return (1 - (distance / radius) ** 2) ** 2;
}

/**
 * Точки, между которыми делится действие кадра, — требование 9: кисть ушла от `from` к `to` дальше
 * четверти `size` — путь режется на равные промежутки не длиннее четверти; `from` уже отработал в
 * прошлом кадре, поэтому в путь не входит.
 */
export function brushPathPoints(from: Vec2 | null, to: Vec2, size: number): Vec2[] {
  const stepLength = size * PATH_STEP_SIZE_FRACTION;
  const distance = from === null ? 0 : Math.hypot(to[0] - from[0], to[1] - from[1]);
  if (from === null || distance <= stepLength) return [to];
  const steps = Math.ceil(distance / stepLength);
  return Array.from({ length: steps }, (_, index): Vec2 => {
    const share = (index + 1) / steps;
    return [from[0] + (to[0] - from[0]) * share, from[1] + (to[1] - from[1]) * share];
  });
}

/** Что кисть делает за кадр: сколько секунд, куда сдвигает («Поднять» — вверх или вниз) и к какой высоте ведёт «Выровнять». */
export type BrushFrame = { settings: BrushSettings; seconds: number; isLowering: boolean; levelTarget: number };

/** Среднее высоты точки и всех её соседей по сетке, до восьми; за краем сетки соседей нет. */
function neighbourhoodMean(heights: Float64Array, columns: number, rows: number, column: number, row: number): number {
  let sum = 0;
  let count = 0;
  for (let neighbourRow = Math.max(0, row - 1); neighbourRow <= Math.min(rows - 1, row + 1); neighbourRow += 1) {
    for (let neighbourColumn = Math.max(0, column - 1); neighbourColumn <= Math.min(columns - 1, column + 1); neighbourColumn += 1) {
      sum += heights[neighbourRow * columns + neighbourColumn] as number;
      count += 1;
    }
  }
  return sum / count;
}

/** Точка высот идёт к цели по закону `h ← цель + (h − цель) × e^(−k × w × dt)` — требования 12–13. */
function approach(height: number, target: number, rate: number): number {
  return target + (height - target) * Math.exp(-rate);
}

/**
 * Действие кисти за один кадр на сетке высот, на месте — «Кисти рельефа», требования 9–13. Секунды кадра
 * делятся поровну между точками пути; меняются только точки сетки в пределах сцены. «Сгладить»
 * берёт средние по высотам до кадра, поэтому порядок обхода точек результат не меняет.
 */
export function applyBrushFrame(grid: TerrainGrid, path: readonly Vec2[], frame: BrushFrame): void {
  const { kind, size, strength } = frame.settings;
  const seconds = Math.min(Math.max(frame.seconds, 0), MAX_FRAME_SECONDS) / path.length;
  const radius = size / 2;
  const before = kind === "smooth" ? Float64Array.from(grid.heights) : grid.heights;
  const direction = frame.isLowering ? -1 : 1;

  for (const [x, y] of path) {
    const firstColumn = Math.max(0, Math.ceil((x - radius) * 2));
    const lastColumn = Math.min(grid.columns - 1, Math.floor((x + radius) * 2));
    const firstRow = Math.max(0, Math.ceil((y - radius) * 2));
    const lastRow = Math.min(grid.rows - 1, Math.floor((y + radius) * 2));
    for (let row = firstRow; row <= lastRow; row += 1) {
      for (let column = firstColumn; column <= lastColumn; column += 1) {
        const weight = brushWeight(Math.hypot(column / 2 - x, row / 2 - y), radius);
        if (weight === 0) continue;
        const index = row * grid.columns + column;
        const height = grid.heights[index] as number;
        if (kind === "raise") {
          grid.heights[index] = height + direction * weight * (strength / RAISE_STRENGTH_DIVISOR) * seconds;
        } else {
          const rate = (strength / LEVEL_STRENGTH_DIVISOR) * weight * seconds;
          const target = kind === "level" ? frame.levelTarget : neighbourhoodMean(before, grid.columns, grid.rows, column, row);
          grid.heights[index] = approach(height, target, rate);
        }
      }
    }
  }
}
