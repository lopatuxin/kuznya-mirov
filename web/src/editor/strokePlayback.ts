import type { Vec2 } from "./objectPlacement";

/** Кадры мазка команды идут по 1/60 секунды, как кадры страницы. */
const STROKE_FRAMES_PER_SECOND = 60;
/** Запас на ошибку счёта с плавающей точкой: ровно `N / 60` секунд — ровно `N` кадров. */
const FRAME_COUNT_EPSILON = 1e-9;

export type StrokeFrame = { point: Vec2; seconds: number };

/** Место на ломаной после доли `share` её длины, `0..1`; у ломаной без длины — первая точка. */
function pointAtShare(points: readonly Vec2[], share: number): Vec2 {
  const lengths = points.slice(1).map((point, index) => Math.hypot(point[0] - (points[index] as Vec2)[0], point[1] - (points[index] as Vec2)[1]));
  const total = lengths.reduce((sum, length) => sum + length, 0);
  if (total === 0) return points[0] as Vec2;
  let remaining = share * total;
  for (const [index, length] of lengths.entries()) {
    if (remaining <= length || index === lengths.length - 1) {
      const from = points[index] as Vec2;
      const to = points[index + 1] as Vec2;
      const part = length === 0 ? 0 : Math.min(remaining / length, 1);
      return [from[0] + (to[0] - from[0]) * part, from[1] + (to[1] - from[1]) * part];
    }
    remaining -= length;
  }
  return points[0] as Vec2;
}

/**
 * Кадры мазка команды — «Кисти», «Мазки командой», требование 24: кисть проходит ломаную `points` с постоянной
 * скоростью ровно за `seconds`, кадры по 1/60 секунды, последний короче; точка кадра — место на ломаной в конце
 * его. Одна точка — кисть стоит на месте. Начало мазка (первая точка) в кадры не входит: кисть уже стоит в ней.
 */
export function strokeFrames(points: readonly Vec2[], seconds: number): StrokeFrame[] {
  const count = Math.max(1, Math.ceil(seconds * STROKE_FRAMES_PER_SECOND - FRAME_COUNT_EPSILON));
  return Array.from({ length: count }, (_, index): StrokeFrame => {
    const start = index / STROKE_FRAMES_PER_SECOND;
    const end = Math.min((index + 1) / STROKE_FRAMES_PER_SECOND, seconds);
    return { point: pointAtShare(points, end / seconds), seconds: end - start };
  });
}
