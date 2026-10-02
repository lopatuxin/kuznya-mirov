import type { Vec2 } from "./objectPlacement";
import { BRUSH_SIZE_LIMITS, BRUSH_STRENGTH_LIMITS, type BrushKind } from "./terrainBrush";

/** Кисти файла мазков: кисти рельефа и «Покрасить» — «Кисти», «Мазки командой». «Размыть» (`erode`) придёт в Фазе 22. */
export type StrokeBrush = BrushKind | "paint";

/** Мазок файла мазков: что редактор получил бы от мыши, записано числами. */
export type BrushStroke = {
  brush: StrokeBrush;
  size: number;
  strength: number;
  seconds: number;
  points: Vec2[];
  /** Как с Shift: у «Поднять» опускает, у «Покрасить» стирает, у остальных ничего не меняет. */
  isShift: boolean;
  /** Материал «Покрасить»; у кистей рельефа `null`. */
  material: string | null;
};

export type StrokeFileResult = { status: "ok"; strokes: BrushStroke[] } | { status: "error"; message: string };

const STROKE_BRUSHES: readonly string[] = ["raise", "level", "smooth", "paint"];
const REQUIRED_KEYS = ["brush", "size", "strength", "seconds", "points"];
const ALLOWED_KEYS = [...REQUIRED_KEYS, "shift", "material"];

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isPoint(value: unknown): value is Vec2 {
  return Array.isArray(value) && value.length === 2 && value.every(isFiniteNumber);
}

type FieldError = { field: string; message: string };

/** Первая ошибка мазка — поле и что с ним не так; `null` — мазок в порядке. */
function findFieldError(stroke: Record<string, unknown>, materials: readonly string[]): FieldError | null {
  const unknownKey = Object.keys(stroke).find((key) => !ALLOWED_KEYS.includes(key));
  if (unknownKey !== undefined) return { field: unknownKey, message: `неизвестный ключ; есть ${ALLOWED_KEYS.join(", ")}` };
  const missing = REQUIRED_KEYS.find((key) => !(key in stroke));
  if (missing !== undefined) return { field: missing, message: "обязательного поля нет" };
  const { brush, size, strength, seconds, points, shift, material } = stroke;
  if (typeof brush !== "string" || !STROKE_BRUSHES.includes(brush)) {
    return { field: "brush", message: `нужна одна из кистей: ${STROKE_BRUSHES.join(", ")}${brush === "erode" ? " (erode придёт в Фазе 22)" : ""}` };
  }
  if (!isFiniteNumber(size) || size < BRUSH_SIZE_LIMITS.min || size > BRUSH_SIZE_LIMITS.max) return { field: "size", message: `нужно число от ${BRUSH_SIZE_LIMITS.min} до ${BRUSH_SIZE_LIMITS.max}` };
  if (!isFiniteNumber(strength) || strength < BRUSH_STRENGTH_LIMITS.min || strength > BRUSH_STRENGTH_LIMITS.max) {
    return { field: "strength", message: `нужно число от ${BRUSH_STRENGTH_LIMITS.min} до ${BRUSH_STRENGTH_LIMITS.max}` };
  }
  if (!isFiniteNumber(seconds) || seconds <= 0) return { field: "seconds", message: "нужно число больше нуля" };
  if (!Array.isArray(points) || points.length === 0 || !points.every(isPoint)) return { field: "points", message: "нужен непустой список пар чисел [x, y]" };
  if (shift !== undefined && typeof shift !== "boolean") return { field: "shift", message: "нужно true или false" };
  if (brush !== "paint") return material === undefined ? null : { field: "material", message: "материал есть только у paint" };
  if (typeof material !== "string") return { field: "material", message: "у paint нужна строка с материалом из files.materials" };
  return materials.includes(material) ? null : { field: "material", message: `материала «${material}» нет в files.materials` };
}

/**
 * Файл мазков — «Кисти», «Мазки командой», `npm run stroke`, требование 23: список объектов
 * `{brush, size, strength, seconds, points, shift?, material?}`. Первая же ошибка называет мазок с единицы и поле;
 * `materials` — имена из `files.materials`.
 */
export function parseStrokeFile(text: string, materials: readonly string[]): StrokeFileResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    return { status: "error", message: `файл мазков не разбирается как JSON: ${error instanceof Error ? error.message : String(error)}` };
  }
  if (!Array.isArray(parsed)) return { status: "error", message: "файл мазков: ожидается список мазков" };
  const strokes: BrushStroke[] = [];
  for (const [index, item] of parsed.entries()) {
    if (item === null || typeof item !== "object" || Array.isArray(item)) return { status: "error", message: `мазок ${index + 1}: ожидается объект` };
    const stroke = item as Record<string, unknown>;
    const error = findFieldError(stroke, materials);
    if (error !== null) return { status: "error", message: `мазок ${index + 1} → ${error.field}: ${error.message}` };
    strokes.push({
      brush: stroke.brush as StrokeBrush,
      size: stroke.size as number,
      strength: stroke.strength as number,
      seconds: stroke.seconds as number,
      points: (stroke.points as Vec2[]).map(([x, y]): Vec2 => [x, y]),
      isShift: stroke.shift === true,
      material: stroke.brush === "paint" ? (stroke.material as string) : null,
    });
  }
  return { status: "ok", strokes };
}
