import { ARROW_LENGTH_PX, type HandleGeometry } from "./handleGeometry";
import type { Vec2 } from "./objectPlacement";
import type { CanvasRect } from "./selectionDrawing";

/** Ручки масштаба плоской сцены — «Редактор», «Правка сцены», требование 18: четыре стороны и четыре угла рамки объекта. */
export type FlatScaleHandle = "left" | "right" | "top" | "bottom" | "top-left" | "top-right" | "bottom-left" | "bottom-right";

/** Сторона рамки, которую держит ручка, по каждой оси: −1 — левая или верхняя, 1 — правая или нижняя, 0 — ось ручка не двигает. */
export const SCALE_HANDLE_SIDES: Record<FlatScaleHandle, readonly [-1 | 0 | 1, -1 | 0 | 1]> = {
  left: [-1, 0],
  right: [1, 0],
  top: [0, -1],
  bottom: [0, 1],
  "top-left": [-1, -1],
  "top-right": [1, -1],
  "bottom-left": [-1, 1],
  "bottom-right": [1, 1],
};

/** Сторона квадратика ручки масштаба на экране — «Редактор», требование 18. */
export const SCALE_HANDLE_SIZE_PX = 8;
/** Попадание — в 8 точках от ручки, «Редактор», требование 21. */
const SCALE_HIT_TOLERANCE_PX = 8;

const CORNER_HANDLES: FlatScaleHandle[] = ["top-left", "top-right", "bottom-left", "bottom-right"];
const SIDE_HANDLES: FlatScaleHandle[] = ["left", "right", "top", "bottom"];

/** Ручки переноса плоской сцены — «Редактор», требование 17: стрелки вправо и вниз длиной 90 точек и квадрат в середине прямоугольника. */
export function computeFlatTranslateGeometry(rect: CanvasRect): HandleGeometry {
  const center: Vec2 = [rect.x + rect.width / 2, rect.y + rect.height / 2];
  return { center, tipX: [center[0] + ARROW_LENGTH_PX, center[1]], tipY: [center[0], center[1] + ARROW_LENGTH_PX], tipZ: null, ring: [] };
}

/** Где на экране каждая ручка масштаба: на рамке выбранного объекта. */
export function flatScaleHandlePoints(rect: CanvasRect): Record<FlatScaleHandle, Vec2> {
  const left = rect.x;
  const right = rect.x + rect.width;
  const top = rect.y;
  const bottom = rect.y + rect.height;
  const middleX = rect.x + rect.width / 2;
  const middleY = rect.y + rect.height / 2;
  return {
    left: [left, middleY],
    right: [right, middleY],
    top: [middleX, top],
    bottom: [middleX, bottom],
    "top-left": [left, top],
    "top-right": [right, top],
    "bottom-left": [left, bottom],
    "bottom-right": [right, bottom],
  };
}

function nearestHandleWithin(points: Record<FlatScaleHandle, Vec2>, candidates: FlatScaleHandle[], point: Vec2): FlatScaleHandle | null {
  let best: FlatScaleHandle | null = null;
  let bestDistance = Infinity;
  for (const handle of candidates) {
    const [x, y] = points[handle];
    if (Math.abs(point[0] - x) > SCALE_HIT_TOLERANCE_PX || Math.abs(point[1] - y) > SCALE_HIT_TOLERANCE_PX) continue;
    const distance = Math.hypot(point[0] - x, point[1] - y);
    if (distance < bestDistance) {
      best = handle;
      bestDistance = distance;
    }
  }
  return best;
}

/** Ручка масштаба под точкой холста — «Редактор», требование 21: угол важнее стороны; из равных берётся ближняя. */
export function hitTestFlatScaleHandles(rect: CanvasRect, point: Vec2): FlatScaleHandle | null {
  const points = flatScaleHandlePoints(rect);
  return nearestHandleWithin(points, CORNER_HANDLES, point) ?? nearestHandleWithin(points, SIDE_HANDLES, point);
}
