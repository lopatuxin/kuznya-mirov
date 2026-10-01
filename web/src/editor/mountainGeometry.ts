import { computeHandleGeometry, type HandleGeometry, type HandleMode, type SpaceProjection } from "./handleGeometry";
import { computeRotation, computeScale, computeTranslate, trimFloatNoise, type ScaleAxis, type TranslateAxis } from "./handleMath";
import type { ObjectPlacement, Vec2 } from "./objectPlacement";
import { roundToHundredths, type MountainEntry } from "./terrainFile";

/** Гора, у которой все поля — числа: её можно показать ручками и двигать. `position` — середина, `rotation` — градусы, 0 по умолчанию. */
export type Mountain = { stamp: string; position: Vec2; size: Vec2; height: number; rotation: number };

/** Штамп из `files.stamps`: имя и число точек в строке и строк — по ним берётся глубина новой горы. */
export type StampShape = { name: string; columns: number; rows: number };

function readFiniteVec2(value: unknown): Vec2 | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const [x, y] = value as unknown[];
  return typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y) ? [x, y] : null;
}

/** Гора из файла для ручек; не хватает поля или значение не число — `null`: такую гору движок назовёт ошибкой, ручек у неё нет. */
export function readMountain(entry: MountainEntry | undefined): Mountain | null {
  if (entry === undefined) return null;
  const position = readFiniteVec2(entry.position);
  const size = readFiniteVec2(entry.size);
  const { stamp, height, rotation } = entry;
  if (typeof stamp !== "string" || position === null || size === null) return null;
  if (typeof height !== "number" || !Number.isFinite(height)) return null;
  if (rotation !== undefined && (typeof rotation !== "number" || !Number.isFinite(rotation))) return null;
  return { stamp, position, size, height, rotation: rotation ?? 0 };
}

/** Гора в виде файла: числа до сотых, `rotation` — только если не 0. */
export function mountainToEntry(mountain: Mountain): MountainEntry {
  const entry: MountainEntry = {
    stamp: mountain.stamp,
    position: [roundToHundredths(mountain.position[0]), roundToHundredths(mountain.position[1])],
    size: [roundToHundredths(mountain.size[0]), roundToHundredths(mountain.size[1])],
    height: roundToHundredths(mountain.height),
  };
  const rotation = roundToHundredths(mountain.rotation);
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
 * Новая гора — «Редактор», «Правка сцены», кнопка «Гора»: середина — место на земле, ширина — из поля,
 * глубина — по пропорции штампа (`ширина × строк / точек в строке`), `rotation` не пишется.
 */
export function newMountainEntry(place: Vec2, shape: StampShape, width: number, height: number): MountainEntry {
  return mountainToEntry({ stamp: shape.name, position: place, size: [width, (width * shape.rows) / shape.columns], height, rotation: 0 });
}

/** Копия горы со сдвигом на клетку по `x` — «Редактор», копия. Гора, у которой нет `position` из двух чисел, копируется как есть. */
export function shiftedMountainCopy(entry: MountainEntry): MountainEntry {
  const position = readFiniteVec2(entry.position);
  return position === null ? { ...entry } : { ...entry, position: [position[0] + 1, position[1]] };
}

/** Гора как объект ручек: прямоугольник от левого верхнего угла, у которого есть высота. Середина та же, что `position` горы. */
export function mountainAsPlacement(mountain: Mountain): ObjectPlacement {
  return {
    position: [mountain.position[0] - mountain.size[0] / 2, mountain.position[1] - mountain.size[1] / 2],
    z: null,
    size: mountain.size,
    height: mountain.height,
    rotation: mountain.rotation,
    hasShape: true,
  };
}

/** Ручки горы: у горы нет вертикальной стрелки переноса, остальное — как у объекта с высотой. */
export function mountainHandleGeometry(projection: SpaceProjection, mountain: Mountain, mode: HandleMode, baseHeight: number): HandleGeometry | undefined {
  const geometry = computeHandleGeometry(projection, mountainAsPlacement(mountain), mode, baseHeight);
  return geometry === undefined || mode !== "translate" ? geometry : { ...geometry, tipZ: null };
}

/** Перенос середины: `delta` — на сколько ушло место под указателем; с Ctrl середина встаёт на целые клетки. */
export function translatedMountain(start: Mountain, delta: Vec2, axis: TranslateAxis, snapToWholeCells: boolean): Mountain {
  return { ...start, position: computeTranslate(start.position, delta, axis, snapToWholeCells) };
}

/** Поворот вокруг середины на угол, на который ушло место под указателем; в отрезке от 0 до 360, с Ctrl — кратно 15°. */
export function turnedMountain(start: Mountain, groundStart: Vec2, groundNow: Vec2, snapToStep: boolean): Mountain {
  return { ...start, rotation: computeRotation(start.rotation, start.position, groundStart, groundNow, snapToStep) };
}

/** Масштаб — середина остаётся на месте, размеры и высота не меньше 0,1 клетки, с Ctrl доля — до десятой. */
export function scaledMountain(start: Mountain, axis: ScaleAxis, factor: number, snapFactorToTenths: boolean): Mountain {
  const scaled = computeScale(mountainAsPlacement(start), axis, factor, snapFactorToTenths);
  return {
    ...start,
    position: [trimFloatNoise(scaled.position[0] + scaled.size[0] / 2), trimFloatNoise(scaled.position[1] + scaled.size[1] / 2)],
    size: scaled.size,
    height: scaled.height ?? start.height,
  };
}

/**
 * Рамка горы — «Лепка рельефа», «Редактор»: углы повёрнутого прямоугольника и точки по его сторонам не
 * реже чем через клетку, по порядку обхода; высоту каждой точки берёт видимая земля.
 */
export function mountainOutline(mountain: Mountain): Vec2[] {
  const radians = (mountain.rotation * Math.PI) / 180;
  const [cos, sin] = [Math.cos(radians), Math.sin(radians)];
  const [halfWidth, halfDepth] = [mountain.size[0] / 2, mountain.size[1] / 2];
  const corner = (along: number, across: number): Vec2 => [
    mountain.position[0] + cos * along - sin * across,
    mountain.position[1] + sin * along + cos * across,
  ];
  const corners = [corner(-halfWidth, -halfDepth), corner(halfWidth, -halfDepth), corner(halfWidth, halfDepth), corner(-halfWidth, halfDepth)];
  return corners.flatMap((from, index) => {
    const to = corners[(index + 1) % corners.length] as Vec2;
    const steps = Math.max(1, Math.ceil(Math.hypot(to[0] - from[0], to[1] - from[1])));
    return Array.from({ length: steps }, (_, step): Vec2 => [from[0] + ((to[0] - from[0]) * step) / steps, from[1] + ((to[1] - from[1]) * step) / steps]);
  });
}
