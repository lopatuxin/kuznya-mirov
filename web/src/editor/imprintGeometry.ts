import { computeHandleGeometry, type HandleGeometry, type HandleMode, type SpaceProjection } from "./handleGeometry";
import { computeRotation, computeScale, computeTranslate, trimFloatNoise, type ScaleAxis, type TranslateAxis } from "./handleMath";
import type { ObjectPlacement, Vec2 } from "./objectPlacement";
import { roundToHundredths, type ImprintEntry } from "./terrainFile";

/** Отпечаток, у которого все поля — числа: его можно показать ручками и двигать. `position` — середина, `rotation` — градусы, 0 по умолчанию. */
export type Imprint = { stamp: string; position: Vec2; size: Vec2; height: number; rotation: number };

/** Штамп из `files.stamps`: имя и число точек в строке и строк — по ним берётся глубина нового отпечатка. */
export type StampShape = { name: string; columns: number; rows: number };

function readFiniteVec2(value: unknown): Vec2 | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const [x, y] = value as unknown[];
  return typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y) ? [x, y] : null;
}

/** Отпечаток из файла для ручек; не хватает поля или значение не число — `null`: такой отпечаток движок назовёт ошибкой, ручек у него нет. */
export function readImprint(entry: ImprintEntry | undefined): Imprint | null {
  if (entry === undefined) return null;
  const position = readFiniteVec2(entry.position);
  const size = readFiniteVec2(entry.size);
  const { stamp, height, rotation } = entry;
  if (typeof stamp !== "string" || position === null || size === null) return null;
  if (typeof height !== "number" || !Number.isFinite(height)) return null;
  if (rotation !== undefined && (typeof rotation !== "number" || !Number.isFinite(rotation))) return null;
  return { stamp, position, size, height, rotation: rotation ?? 0 };
}

/** Отпечаток в виде файла: числа до сотых, `rotation` — только если не 0. */
export function imprintToEntry(imprint: Imprint): ImprintEntry {
  const entry: ImprintEntry = {
    stamp: imprint.stamp,
    position: [roundToHundredths(imprint.position[0]), roundToHundredths(imprint.position[1])],
    size: [roundToHundredths(imprint.size[0]), roundToHundredths(imprint.size[1])],
    height: roundToHundredths(imprint.height),
  };
  const rotation = roundToHundredths(imprint.rotation);
  return rotation === 0 ? entry : { ...entry, rotation };
}

/** Штамп из текста его файла: строки сверху вниз; текст, что не сетка, — `null` (проверять его — дело движка). */
export function parseStampShape(name: string, text: string | null): StampShape | null {
  if (text === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const rows = (parsed as { heights?: unknown } | null)?.heights;
  if (!Array.isArray(rows) || rows.length === 0 || !Array.isArray(rows[0]) || rows[0].length === 0) return null;
  return { name, columns: rows[0].length, rows: rows.length };
}

/**
 * Новый отпечаток — «Редактор», «Правка сцены», кнопка «Отпечаток»: середина — место на земле, ширина — из поля,
 * глубина — по пропорции штампа (`ширина × строк / точек в строке`), `rotation` не пишется.
 */
export function newImprintEntry(place: Vec2, shape: StampShape, width: number, height: number): ImprintEntry {
  return imprintToEntry({ stamp: shape.name, position: place, size: [width, (width * shape.rows) / shape.columns], height, rotation: 0 });
}

/** Копия отпечатка со сдвигом на клетку по `x` — «Редактор», копия. Отпечаток, у которого нет `position` из двух чисел, копируется как есть. */
export function shiftedImprintCopy(entry: ImprintEntry): ImprintEntry {
  const position = readFiniteVec2(entry.position);
  return position === null ? { ...entry } : { ...entry, position: [position[0] + 1, position[1]] };
}

/** Отпечаток как объект ручек: прямоугольник от левого верхнего угла, у которого есть высота. Середина та же, что `position` отпечатка. */
export function imprintAsPlacement(imprint: Imprint): ObjectPlacement {
  return {
    position: [imprint.position[0] - imprint.size[0] / 2, imprint.position[1] - imprint.size[1] / 2],
    z: null,
    size: imprint.size,
    height: imprint.height,
    rotation: imprint.rotation,
    hasShape: true,
  };
}

/**
 * Ручки отпечатка: у отпечатка нет вертикальной стрелки переноса, остальное — как у объекта с высотой;
 * у вдавливающего отпечатка (высота меньше нуля) стрелка высоты масштаба смотрит вниз.
 */
export function imprintHandleGeometry(projection: SpaceProjection, imprint: Imprint, mode: HandleMode, baseHeight: number): HandleGeometry | undefined {
  const geometry = computeHandleGeometry(projection, imprintAsPlacement(imprint), mode, baseHeight);
  if (geometry === undefined) return geometry;
  if (mode === "translate") return { ...geometry, tipZ: null };
  if (mode === "scale" && imprint.height < 0 && geometry.tipZ !== null) {
    return { ...geometry, tipZ: [2 * geometry.center[0] - geometry.tipZ[0], 2 * geometry.center[1] - geometry.tipZ[1]] };
  }
  return geometry;
}

/** Перенос середины: `delta` — на сколько ушло место под указателем; с Ctrl середина встаёт на целые клетки. */
export function translatedImprint(start: Imprint, delta: Vec2, axis: TranslateAxis, snapToWholeCells: boolean): Imprint {
  return { ...start, position: computeTranslate(start.position, delta, axis, snapToWholeCells) };
}

/** Поворот вокруг середины на угол, на который ушло место под указателем; в отрезке от 0 до 360, с Ctrl — кратно 15°. */
export function turnedImprint(start: Imprint, groundStart: Vec2, groundNow: Vec2, snapToStep: boolean): Imprint {
  return { ...start, rotation: computeRotation(start.rotation, start.position, groundStart, groundNow, snapToStep) };
}

/** Масштаб — середина остаётся на месте, размеры и |высота| не меньше 0,1 клетки, знак высоты сохраняется, с Ctrl доля — до десятой. */
export function scaledImprint(start: Imprint, axis: ScaleAxis, factor: number, snapFactorToTenths: boolean): Imprint {
  const scaled = computeScale({ ...imprintAsPlacement(start), height: Math.abs(start.height) }, axis, factor, snapFactorToTenths);
  const sign = start.height < 0 ? -1 : 1;
  return {
    ...start,
    position: [trimFloatNoise(scaled.position[0] + scaled.size[0] / 2), trimFloatNoise(scaled.position[1] + scaled.size[1] / 2)],
    size: scaled.size,
    height: sign * (scaled.height ?? Math.abs(start.height)),
  };
}

/**
 * Рамка отпечатка — «Лепка рельефа», «Редактор»: углы повёрнутого прямоугольника и точки по его сторонам не
 * реже чем через клетку, по порядку обхода; высоту каждой точки берёт видимая земля.
 */
export function imprintOutline(imprint: Imprint): Vec2[] {
  const radians = (imprint.rotation * Math.PI) / 180;
  const [cos, sin] = [Math.cos(radians), Math.sin(radians)];
  const [halfWidth, halfDepth] = [imprint.size[0] / 2, imprint.size[1] / 2];
  const corner = (along: number, across: number): Vec2 => [
    imprint.position[0] + cos * along - sin * across,
    imprint.position[1] + sin * along + cos * across,
  ];
  const corners = [corner(-halfWidth, -halfDepth), corner(halfWidth, -halfDepth), corner(halfWidth, halfDepth), corner(-halfWidth, halfDepth)];
  return corners.flatMap((from, index) => {
    const to = corners[(index + 1) % corners.length] as Vec2;
    const steps = Math.max(1, Math.ceil(Math.hypot(to[0] - from[0], to[1] - from[1])));
    return Array.from({ length: steps }, (_, step): Vec2 => [from[0] + ((to[0] - from[0]) * step) / steps, from[1] + ((to[1] - from[1]) * step) / steps]);
  });
}
