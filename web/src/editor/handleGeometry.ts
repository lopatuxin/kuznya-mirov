import type { ScaleAxis } from "./handleMath";
import { placementCenter, type ObjectPlacement, type Vec2 } from "./objectPlacement";

export type HandleMode = "translate" | "rotate" | "scale";

/** Что схвачено: ось сцены (перенос) или ось вдоль стороны (масштаб), середина, кольцо. */
export type HandleHit = "axis-x" | "axis-y" | "axis-z" | "center" | "ring";

/** Вызов движка, из которого строятся ручки: точка холста для места сцены (клетки) на высоте `z` (клетки). */
export type SpaceProjection = {
  screenPoint(x: number, y: number, z: number): Vec2 | undefined;
};

/** Ручки на экране: середина объекта на земле и то, что нужно режиму. Все точки — CSS-пиксели холста. */
export type HandleGeometry = {
  center: Vec2;
  /** Конец стрелки по `x` сцены (перенос) или вдоль ширины объекта (масштаб). */
  tipX: Vec2 | null;
  /** Конец стрелки по `y` сцены (перенос) или вдоль глубины объекта (масштаб). */
  tipY: Vec2 | null;
  /** Конец стрелки высоты: вертикальная стрелка переноса основания или высота фигуры при масштабе. */
  tipZ: Vec2 | null;
  /** Кольцо поворота ломаной по кругу. */
  ring: Vec2[];
};

/** «Редактор», требование 9: размеры ручек в точках экрана, одни при любом приближении. */
export const ARROW_LENGTH_PX = 90;
const RING_RADIUS_PX = 80;
export const CENTER_HALF_SIZE_PX = 8;
/** Стрелка начинается за средним квадратом: нажатие в его пределах — это середина, а не ось. */
const ARROW_START_PX = 14;
const HIT_TOLERANCE_PX = 8;
const RING_SEGMENTS = 48;
const JACOBIAN_STEP_CELLS = 0.05;

function subtract(a: Vec2, b: Vec2): Vec2 {
  return [a[0] - b[0], a[1] - b[1]];
}

/**
 * Сколько клеток земли в одной точке экрана на строке середины объекта, стоящей на высоте `height` — «Технические детали»:
 * образ шага по осям земли даёт матрицу «клетки → точки экрана», обратная к ней переводит точку
 * экрана вправо в клетки. На строке экрана расстояние до камеры вдоль взгляда одно, поэтому число
 * постоянно вдоль неё. Только `screen_point`: `ground_at` кадр не зовёт. `undefined`, если шаг не
 * виден камерой.
 */
export function cellsPerScreenPixel(projection: SpaceProjection, ground: Vec2, center: Vec2, height: number): number | undefined {
  const stepX = projection.screenPoint(ground[0] + JACOBIAN_STEP_CELLS, ground[1], height);
  const stepY = projection.screenPoint(ground[0], ground[1] + JACOBIAN_STEP_CELLS, height);
  if (stepX === undefined || stepY === undefined) return undefined;
  const ax = (stepX[0] - center[0]) / JACOBIAN_STEP_CELLS;
  const ay = (stepX[1] - center[1]) / JACOBIAN_STEP_CELLS;
  const bx = (stepY[0] - center[0]) / JACOBIAN_STEP_CELLS;
  const by = (stepY[1] - center[1]) / JACOBIAN_STEP_CELLS;
  const determinant = ax * by - bx * ay;
  if (Math.abs(determinant) < 1e-9) return undefined;
  return Math.hypot(by, ay) / Math.abs(determinant);
}

/**
 * Конец вертикальной стрелки переноса — «Рельеф», требование 42: вверх из середины на экране, той же
 * длины в точках, что стрелки по земле. `undefined`, если точка выше середины не видна или вертикаль
 * на экране нулевой длины (взгляд строго вниз).
 */
export function verticalUnitOnScreen(projection: SpaceProjection, ground: Vec2, center: Vec2, baseHeight: number): Vec2 | undefined {
  const above = projection.screenPoint(ground[0], ground[1], baseHeight + 1);
  if (above === undefined) return undefined;
  const unit = subtract(above, center);
  return Math.hypot(unit[0], unit[1]) < 1e-6 ? undefined : unit;
}

function verticalTip(projection: SpaceProjection, ground: Vec2, center: Vec2, baseHeight: number): Vec2 | null {
  const unit = verticalUnitOnScreen(projection, ground, center, baseHeight);
  if (unit === undefined) return null;
  const scale = ARROW_LENGTH_PX / Math.hypot(unit[0], unit[1]);
  return [center[0] + unit[0] * scale, center[1] + unit[1] * scale];
}

/**
 * Геометрия ручек на экране для режима — «Редактор», требование 9. Перенос — стрелки по осям сцены
 * и зелёная вертикальная; поворот — кольцо (эллипс по двум осям земли, как круг виден камерой);
 * масштаб — стрелки вдоль сторон объекта с его поворотом и, у фигуры, вверх. Середина и все стрелки
 * — на высоте основания `baseHeight` («Рельеф», требование 40). `undefined`, если середину не видно
 * камерой или шаг от неё по земле не виден.
 */
export function computeHandleGeometry(projection: SpaceProjection, placement: ObjectPlacement, mode: HandleMode, baseHeight: number): HandleGeometry | undefined {
  const ground = placementCenter(placement);
  const center = projection.screenPoint(ground[0], ground[1], baseHeight);
  if (center === undefined) return undefined;
  const cellsPerPixel = cellsPerScreenPixel(projection, ground, center, baseHeight);
  if (cellsPerPixel === undefined) return undefined;
  const length = ARROW_LENGTH_PX * cellsPerPixel;
  const tipAlong = (direction: Vec2): Vec2 | null =>
    projection.screenPoint(ground[0] + direction[0] * length, ground[1] + direction[1] * length, baseHeight) ?? null;

  if (mode === "translate") {
    return { center, tipX: tipAlong([1, 0]), tipY: tipAlong([0, 1]), tipZ: verticalTip(projection, ground, center, baseHeight), ring: [] };
  }
  if (mode === "rotate") {
    const ringX = tipAlong([1, 0]);
    const ringY = tipAlong([0, 1]);
    if (ringX === null || ringY === null) return undefined;
    return { center, tipX: null, tipY: null, tipZ: null, ring: ringPolyline(center, subtract(ringX, center), subtract(ringY, center)) };
  }
  const radians = ((placement.rotation ?? 0) * Math.PI) / 180;
  const width: Vec2 = [Math.cos(radians), Math.sin(radians)];
  const depth: Vec2 = [-Math.sin(radians), Math.cos(radians)];
  const tipZ = placement.hasShape ? (projection.screenPoint(ground[0], ground[1], baseHeight + length) ?? null) : null;
  return { center, tipX: tipAlong(width), tipY: tipAlong(depth), tipZ, ring: [] };
}

/** Кольцо радиусом `RING_RADIUS_PX` в точках экрана: образ единичной окружности земли по двум её осям. */
function ringPolyline(center: Vec2, axisX: Vec2, axisY: Vec2): Vec2[] {
  const scale = RING_RADIUS_PX / ARROW_LENGTH_PX;
  const points: Vec2[] = [];
  for (let index = 0; index < RING_SEGMENTS; index++) {
    const angle = (index / RING_SEGMENTS) * 2 * Math.PI;
    const cos = Math.cos(angle) * scale;
    const sin = Math.sin(angle) * scale;
    points.push([center[0] + cos * axisX[0] + sin * axisY[0], center[1] + cos * axisX[1] + sin * axisY[1]]);
  }
  return points;
}

/** Расстояние от точки до отрезка. */
function distanceToSegment(point: Vec2, from: Vec2, to: Vec2): number {
  const [dx, dy] = subtract(to, from);
  const lengthSquared = dx * dx + dy * dy;
  const t = lengthSquared === 0 ? 0 : Math.max(0, Math.min(1, ((point[0] - from[0]) * dx + (point[1] - from[1]) * dy) / lengthSquared));
  return Math.hypot(point[0] - (from[0] + t * dx), point[1] - (from[1] + t * dy));
}

function distanceToArrow(point: Vec2, center: Vec2, tip: Vec2 | null): number {
  if (tip === null) return Infinity;
  const [dx, dy] = subtract(tip, center);
  const length = Math.hypot(dx, dy);
  if (length <= ARROW_START_PX) return distanceToSegment(point, center, tip);
  const start: Vec2 = [center[0] + (dx / length) * ARROW_START_PX, center[1] + (dy / length) * ARROW_START_PX];
  return distanceToSegment(point, start, tip);
}

function distanceToRing(point: Vec2, ring: Vec2[]): number {
  let nearest = Infinity;
  for (let index = 0; index < ring.length; index++) {
    const next = ring[(index + 1) % ring.length] as Vec2;
    nearest = Math.min(nearest, distanceToSegment(point, ring[index] as Vec2, next));
  }
  return nearest;
}

/**
 * Ручка под точкой холста — «Редактор», требование 14: середина важнее стрелок, ближняя стрелка
 * важнее дальней; расстояние до стрелки — до её отрезка, до кольца — до его ломаной.
 */
export function hitTestHandles(geometry: HandleGeometry, mode: HandleMode, point: Vec2): HandleHit | null {
  if (mode === "rotate") return distanceToRing(point, geometry.ring) <= HIT_TOLERANCE_PX ? "ring" : null;
  const [dx, dy] = subtract(point, geometry.center);
  if (Math.abs(dx) <= CENTER_HALF_SIZE_PX && Math.abs(dy) <= CENTER_HALF_SIZE_PX) return "center";
  const candidates: [HandleHit, number][] = [
    ["axis-x", distanceToArrow(point, geometry.center, geometry.tipX)],
    ["axis-y", distanceToArrow(point, geometry.center, geometry.tipY)],
    ["axis-z", distanceToArrow(point, geometry.center, geometry.tipZ)],
  ];
  let best: [HandleHit, number] | null = null;
  for (const candidate of candidates) {
    if (candidate[1] <= HIT_TOLERANCE_PX && (best === null || candidate[1] < best[1])) best = candidate;
  }
  return best === null ? null : best[0];
}

/** Ось масштаба для схваченной ручки; ручка не масштаба (кольцо) — `null`. */
export function scaleAxisOfHit(hit: HandleHit): ScaleAxis | null {
  if (hit === "axis-x") return "width";
  if (hit === "axis-y") return "depth";
  if (hit === "axis-z") return "height";
  if (hit === "center") return "uniform";
  return null;
}

/** Вектор от середины к концу стрелки для схваченной оси на экране; `null`, если такой стрелки нет. */
export function arrowVectorOfHit(geometry: HandleGeometry, hit: HandleHit): Vec2 | null {
  const tips: Partial<Record<HandleHit, Vec2 | null>> = { "axis-x": geometry.tipX, "axis-y": geometry.tipY, "axis-z": geometry.tipZ };
  const tip = tips[hit];
  return tip === undefined || tip === null ? null : subtract(tip, geometry.center);
}
