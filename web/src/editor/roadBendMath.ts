import type { Vec2 } from "./objectPlacement";

/** «Дороги дугой по склону», требование 2: изгиб меньше 180°; движок берёт предел 179,9. */
export const MAX_BEND_DEGREES = 179.9;
/** Требование 29: точка ближе этой доли длины хорды к хорде — изгиба нет. */
export const STRAIGHT_CHORD_SHARE = 0.01;
const BEND_HUNDREDTHS = 100;

/** Самый большой изгиб дороги с хордой `chord` и шириной `width`: меньше 180° и радиус дуги не меньше ширины. */
export function maxBend(chord: number, width: number): number {
  if (width <= 0 || chord <= 0) return MAX_BEND_DEGREES;
  const byRadius = (2 * Math.asin(Math.min(chord / (2 * width), 1)) * 180) / Math.PI;
  return Math.min(MAX_BEND_DEGREES, byRadius);
}

/**
 * Изгиб оси, при котором дуга через начало `start` и конец `end` хорды проходит через `point` — «Дороги дугой по
 * склону», требование 29. Положительный изгиб — ось выгибается в сторону `(dy, -dx)` от хода начало → конец, как
 * поворачивает положительный `rotation`. Точка ближе `STRAIGHT_CHORD_SHARE` длины хорды к отрезку хорды — 0, дальше
 * допустимого — ближайшее допустимое. Результат — до сотой доли градуса к нулю: округление в большую сторону вышло бы
 * за предел радиуса, и файл не прошёл бы проверку.
 */
export function bendThroughPoint(start: Vec2, end: Vec2, point: Vec2, width: number): number {
  const chordX = end[0] - start[0];
  const chordY = end[1] - start[1];
  const chord = Math.hypot(chordX, chordY);
  if (chord === 0) return 0;
  const alongX = chordX / chord;
  const alongY = chordY / chord;
  const fromMiddleX = point[0] - (start[0] + end[0]) / 2;
  const fromMiddleY = point[1] - (start[1] + end[1]) / 2;
  const along = fromMiddleX * alongX + fromMiddleY * alongY;
  const aside = fromMiddleX * alongY - fromMiddleY * alongX;
  if (aside === 0 || Math.hypot(Math.max(Math.abs(along) - chord / 2, 0), aside) < chord * STRAIGHT_CHORD_SHARE) return 0;
  // Центр окружности через концы и точку лежит на перпендикуляре к хорде в её середине, на `centerAside` от хорды.
  const centerAside = (along * along + aside * aside - (chord * chord) / 4) / (2 * Math.abs(aside));
  const arcDegrees = (2 * Math.atan2(chord / 2, -centerAside) * 180) / Math.PI;
  const limited = Math.min(arcDegrees, maxBend(chord, width));
  return (Math.sign(aside) * Math.trunc(limited * BEND_HUNDREDTHS)) / BEND_HUNDREDTHS;
}
