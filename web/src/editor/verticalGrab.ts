import type { SpaceProjection } from "./handleGeometry";
import type { Vec2 } from "./objectPlacement";

type Vec3 = readonly [number, number, number];

/** Точка вертикали должна стоять перед камерой: глубина не меньше этой доли глубины середины. */
const MIN_DEPTH_RATIO = 1e-6;
const DEGENERATE = 1e-9;

/** Камера, восстановленная по `screen_point`: обратная матрица 3×3 (строками, с общим определителем) и глаз. */
type RayModel = { eye: Vec3; inverseRows: readonly [Vec3, Vec3, Vec3]; determinant: number };

/**
 * Вертикаль через середину объекта, как её видит камера на начало жеста — «Рельеф», требование 42.
 * Проекция вертикали на экран — прямая, а высота на ней отображается дробно-линейно: точка на высоте
 * `base + t` лежит на `origin + step · t / (1 + depthSlope · t)`. `ray` — камера, из которой можно
 * взять луч указателя и найти точку вертикали, ближайшую к нему в мире; `null`, если камеру по
 * трём осям не восстановить (проекция почти параллельная) — тогда берётся ближайшая на экране.
 */
export type VerticalGrab = {
  origin: Vec2;
  step: Vec2;
  depthSlope: number;
  ray: RayModel | null;
};

type AxisFit = { step: Vec2; depthSlope: number };

function dot2(a: Vec2, b: Vec2): number {
  return a[0] * b[0] + a[1] * b[1];
}

function dot3(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function cross3(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function applyRows(rows: readonly [Vec3, Vec3, Vec3], vector: Vec3): Vec3 {
  return [dot3(rows[0], vector), dot3(rows[1], vector), dot3(rows[2], vector)];
}

/**
 * Отображение «шаг вдоль оси → экран» по точкам на 1 и 2 клетки от основания: точный дробно-линейный
 * закон `origin + step · t / (1 + depthSlope · t)`. `undefined`, если точка не видна или ось на экране
 * — точка (взгляд вдоль неё).
 */
function fitAxis(projection: SpaceProjection, base: Vec3, origin: Vec2, axis: Vec3): AxisFit | undefined {
  const first = projection.screenPoint(base[0] + axis[0], base[1] + axis[1], base[2] + axis[2]);
  const second = projection.screenPoint(base[0] + 2 * axis[0], base[1] + 2 * axis[1], base[2] + 2 * axis[2]);
  if (first === undefined || second === undefined) return undefined;
  const oneCell: Vec2 = [first[0] - origin[0], first[1] - origin[1]];
  const twoCells: Vec2 = [second[0] - origin[0], second[1] - origin[1]];
  const gap: Vec2 = [twoCells[0] - oneCell[0], twoCells[1] - oneCell[1]];
  const gapSquared = dot2(gap, gap);
  if (gapSquared < 1e-12) return undefined;
  const depthSlope = dot2([oneCell[0] - twoCells[0] / 2, oneCell[1] - twoCells[1] / 2], gap) / gapSquared;
  return { step: [oneCell[0] * (1 + depthSlope), oneCell[1] * (1 + depthSlope)], depthSlope };
}

/**
 * Матрица камеры относительно середины: экран `(u, v)` точки `Y` от середины —
 * `(m1·Y + u0, m2·Y + v0) / (m3·Y + 1)`, столбцы `m` по осям сцены берутся из подгонки осей.
 */
function rayModelOf(origin: Vec2, fits: readonly [AxisFit, AxisFit, AxisFit]): RayModel | null {
  const [columnX, columnY, columnZ] = fits.map((fit): Vec3 => [origin[0] * fit.depthSlope + fit.step[0], origin[1] * fit.depthSlope + fit.step[1], fit.depthSlope]) as [Vec3, Vec3, Vec3];
  const inverseRows: [Vec3, Vec3, Vec3] = [cross3(columnY, columnZ), cross3(columnZ, columnX), cross3(columnX, columnY)];
  const determinant = dot3(columnX, inverseRows[0]);
  const scale = Math.sqrt(dot3(columnX, columnX) * dot3(columnY, columnY) * dot3(columnZ, columnZ));
  if (Math.abs(determinant) <= DEGENERATE * scale) return null;
  const [x, y, z] = applyRows(inverseRows, [origin[0], origin[1], 1]);
  return { eye: [-x / determinant, -y / determinant, -z / determinant], inverseRows, determinant };
}

/**
 * Запоминает вертикаль через `ground` от высоты `baseHeight` по семи вызовам `screen_point`.
 * `undefined`, если середина или нужные точки не видны камерой либо вертикаль на экране точка
 * (камера строго сверху): высоту по указателю тогда не найти.
 */
export function createVerticalGrab(projection: SpaceProjection, ground: Vec2, baseHeight: number): VerticalGrab | undefined {
  const base: Vec3 = [ground[0], ground[1], baseHeight];
  const origin = projection.screenPoint(base[0], base[1], base[2]);
  if (origin === undefined) return undefined;
  const vertical = fitAxis(projection, base, origin, [0, 0, 1]);
  if (vertical === undefined) return undefined;
  const alongX = fitAxis(projection, base, origin, [1, 0, 0]);
  const alongY = fitAxis(projection, base, origin, [0, 1, 0]);
  const ray = alongX === undefined || alongY === undefined ? null : rayModelOf(origin, [alongX, alongY, vertical]);
  return { origin, step: vertical.step, depthSlope: vertical.depthSlope, ray };
}

/** Высота над основанием точки вертикали, ближайшей к точке экрана, — до того, как её лучом станет луч в мире. */
function heightOnScreenLine(grab: VerticalGrab, pointer: Vec2): number {
  const along = dot2([pointer[0] - grab.origin[0], pointer[1] - grab.origin[1]], grab.step) / dot2(grab.step, grab.step);
  return along / (1 - grab.depthSlope * along);
}

/** Высота над основанием точки вертикали, ближайшей в мире к лучу через точку экрана (общий перпендикуляр двух прямых). */
function heightNearestToRay(ray: RayModel, pointer: Vec2): number {
  const [x, y, z] = applyRows(ray.inverseRows, [pointer[0], pointer[1], 1]);
  const direction: Vec3 = [x / ray.determinant, y / ray.determinant, z / ray.determinant];
  const lengthSquared = dot3(direction, direction);
  const upward = direction[2];
  const separation = lengthSquared - upward * upward;
  if (separation <= DEGENERATE * lengthSquared) return Number.NaN;
  return (-upward * dot3(direction, ray.eye) + lengthSquared * ray.eye[2]) / separation;
}

/**
 * На сколько клеток над основанием стоит точка вертикали, ближайшая к лучу указателя. `undefined`,
 * если такой точки нет перед камерой: луч параллелен вертикали (указатель в точке схода) или точка
 * ушла за камеру (указатель за точкой схода).
 */
export function raisedAtPointer(grab: VerticalGrab, pointer: Vec2): number | undefined {
  const raised = grab.ray === null ? heightOnScreenLine(grab, pointer) : heightNearestToRay(grab.ray, pointer);
  return Number.isFinite(raised) && 1 + grab.depthSlope * raised > MIN_DEPTH_RATIO ? raised : undefined;
}

/** Камера, из которой можно взять луч указателя и найти точку горизонтальной плоскости, — для переноса отпечатка по земле. */
export type PlaneGrab = { base: Vec3; ray: RayModel };

/**
 * Модель камеры, восстановленная вокруг точки `around` («Рельеф», перенос отпечатка): те же семь вызовов
 * `screen_point`, что у вертикальной стрелки. `undefined`, если камеру по трём осям не восстановить —
 * тогда плоскость луч не найдёт.
 */
export function createPlaneGrab(projection: SpaceProjection, around: Vec3): PlaneGrab | undefined {
  const ray = createVerticalGrab(projection, [around[0], around[1]], around[2])?.ray;
  return ray === null || ray === undefined ? undefined : { base: around, ray };
}

/**
 * Место `(x, y)` горизонтальной плоскости высоты `height`, где её пересекает луч через точку экрана.
 * `undefined`, если луч параллелен плоскости (камера смотрит вдоль земли) или плоскость за камерой.
 * Точка луча — `eye + s · direction`, где `s` — глубина перед камерой, поэтому `s > 0`.
 */
export function pointOnPlane(grab: PlaneGrab, pointer: Vec2, height: number): Vec2 | undefined {
  const { ray, base } = grab;
  const [x, y, z] = applyRows(ray.inverseRows, [pointer[0], pointer[1], 1]);
  const direction: Vec3 = [x / ray.determinant, y / ray.determinant, z / ray.determinant];
  if (Math.abs(direction[2]) <= DEGENERATE * Math.hypot(direction[0], direction[1], direction[2])) return undefined;
  const depth = (height - base[2] - ray.eye[2]) / direction[2];
  if (!(depth > MIN_DEPTH_RATIO)) return undefined;
  return [base[0] + ray.eye[0] + depth * direction[0], base[1] + ray.eye[1] + depth * direction[1]];
}
