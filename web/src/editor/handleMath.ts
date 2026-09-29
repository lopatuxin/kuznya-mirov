import { effectiveHeight, normalizeRotation, placementCenter, type ObjectPlacement, type Vec2 } from "./objectPlacement";

/** Наименьший размер объекта при масштабе — «Редактор», требование 13. */
const MIN_SCALED_SIZE = 0.1;

const ROTATION_CTRL_STEP_DEGREES = 15;

export type TranslateAxis = "x" | "y" | "free";
export type ScaleAxis = "width" | "depth" | "height" | "uniform";

function roundToHundredth(value: number): number {
  return Math.round(value * 100) / 100;
}

/** Убирает хвост машинного округления (`2.9999999999999996`), не трогая доли до миллионных. */
function trimFloatNoise(value: number): number {
  return Math.round(value * 1e6) / 1e6;
}

/**
 * Перенос — «Редактор», требования 11 и 14: `delta` — на сколько ушла точка земли под указателем;
 * по стрелке меняется только её координата, свободно — обе. Свободно результат округляется до сотой
 * клетки, с Ctrl — до целой, как в плоской сцене.
 */
export function computeTranslate(startPosition: Vec2, delta: Vec2, axis: TranslateAxis, snapToWholeCells: boolean): [number, number] {
  const round = (value: number): number => (snapToWholeCells ? Math.round(value) : roundToHundredth(value));
  const x = axis === "y" ? startPosition[0] : round(startPosition[0] + delta[0]);
  const y = axis === "x" ? startPosition[1] : round(startPosition[1] + delta[1]);
  return [x, y];
}

/** Направление на месте `point` от `center` на земле, в градусах: по часовой стрелке, если смотреть сверху, растёт. */
function groundAngleDegrees(center: Vec2, point: Vec2): number {
  return (Math.atan2(point[1] - center[1], point[0] - center[0]) * 180) / Math.PI;
}

/**
 * Поворот — «Редактор», требование 12: `rotation` растёт на угол, на который повернулась точка
 * земли под указателем вокруг середины объекта. Свободно — целый градус, с Ctrl — кратно 15°; в
 * отрезке от 0 до 360. `startRotation` — записанный в файле, у объекта без `rotation` — `null`.
 */
export function computeRotation(
  startRotation: number | null,
  center: Vec2,
  groundStart: Vec2,
  groundNow: Vec2,
  snapToStep: boolean,
): number {
  const turned = groundAngleDegrees(center, groundNow) - groundAngleDegrees(center, groundStart);
  const raw = (startRotation ?? 0) + turned;
  const step = snapToStep ? ROTATION_CTRL_STEP_DEGREES : 1;
  return normalizeRotation(Math.round(raw / step) * step);
}

/**
 * Доля масштаба вдоль ручки — «Редактор», требование 13: расстояние указателя от середины объекта
 * на экране вдоль оси ручки сейчас, делённое на то же расстояние в начале. `axisPx` — вектор от
 * середины к концу ручки на экране. Ручка нулевой длины на экране (взгляд вдоль неё) долю не меняет.
 */
export function scaleFactorAlong(center: Vec2, axisPx: Vec2, pointerStart: Vec2, pointerNow: Vec2): number {
  const length = Math.hypot(axisPx[0], axisPx[1]);
  if (length < 1e-6) return 1;
  const along = (point: Vec2): number => ((point[0] - center[0]) * axisPx[0] + (point[1] - center[1]) * axisPx[1]) / length;
  const alongStart = along(pointerStart);
  if (Math.abs(alongStart) < 1) return 1;
  return along(pointerNow) / alongStart;
}

/**
 * Доля общей ручки, что стоит в середине объекта: расстояния от середины до указателя в начале
 * почти нет, поэтому доля растёт линейно — на длину ручки `referencePx` вправо-вверх по экрану доля
 * растёт на единицу, влево-вниз — падает.
 */
export function scaleFactorUniform(pointerStart: Vec2, pointerNow: Vec2, referencePx: number): number {
  const rightUp = ((pointerNow[0] - pointerStart[0]) - (pointerNow[1] - pointerStart[1])) / Math.SQRT2;
  return 1 + rightUp / referencePx;
}

/**
 * Масштаб — «Редактор», требование 13: размеры оси (или все, у общей ручки) умножаются на долю,
 * середина прямоугольника на земле остаётся на месте, размер не меньше 0,1 клетки. Свободно размеры
 * округляются до сотой клетки, с Ctrl доля — до десятой. У плоского объекта на земле высоты нет.
 */
export function computeScale(start: ObjectPlacement, axis: ScaleAxis, factor: number, snapFactorToTenths: boolean): ObjectPlacement {
  const applied = snapFactorToTenths ? Math.round(factor * 10) / 10 : factor;
  const scaleValue = (value: number): number => {
    const scaled = Math.max(MIN_SCALED_SIZE, value * applied);
    return snapFactorToTenths ? trimFloatNoise(scaled) : Math.max(MIN_SCALED_SIZE, roundToHundredth(scaled));
  };
  const scalesWidth = axis === "width" || axis === "uniform";
  const scalesDepth = axis === "depth" || axis === "uniform";
  const scalesHeight = (axis === "height" || axis === "uniform") && start.hasShape;

  const size: [number, number] = [scalesWidth ? scaleValue(start.size[0]) : start.size[0], scalesDepth ? scaleValue(start.size[1]) : start.size[1]];
  const center = placementCenter(start);
  const positionAlong = (index: 0 | 1): number => (size[index] === start.size[index] ? start.position[index] : trimFloatNoise(center[index] - size[index] / 2));
  const startHeight = effectiveHeight(start);
  const height = scalesHeight && startHeight !== null ? scaleValue(startHeight) : start.height;
  return { ...start, position: [positionAlong(0), positionAlong(1)], size, height };
}
