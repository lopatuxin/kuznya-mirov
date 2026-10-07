import type { ParticleFieldSpec } from "./particleFieldSpecs";
import { parseNumberField } from "./terrainBrush";

/** Значение поля после разбора набранного: число, пара или ряд точек; `undefined` — ключ убирается. */
export type ParticleFieldValue = number | number[] | undefined;

type ParsedParticleField = { isValid: false } | { isValid: true; value: ParticleFieldValue };

/** Текст поля вида: одно поле на настройку, как в наброске вкладки — «4 – 6», «0 · 0,7 · 0». */
type ParticleFieldText = { text: string; isDefault: boolean };

const MINUS = "−";
const RANGE_DASH = "–";
const POINT_SEPARATOR = " · ";

/** Число, как его пишут по-русски: дробная часть через запятую, минус — длинный. */
export function formatParticleNumber(value: number): string {
  return String(value).replace(".", ",").replace(/^-/, MINUS);
}

function textOfValue(value: unknown): string {
  return typeof value === "number" ? formatParticleNumber(value) : JSON.stringify(value);
}

function isNumberList(value: unknown): value is number[] {
  return Array.isArray(value) && value.length > 0 && value.every((item) => typeof item === "number");
}

/**
 * Что показывает поле у значения из файла — «Редактор», требование 26: число — само; пара — «от – до»; ряд точек
 * видимости — через точку. Ключа нет — значение по умолчанию, бледно (`isDefault`), а у обязательного ключа — пусто.
 * Значение не того вида показывается как записано, чтобы его было видно и можно было исправить.
 */
export function particleFieldText(spec: ParticleFieldSpec, value: unknown): ParticleFieldText {
  if (value === undefined) return { text: spec.defaultValue === undefined ? "" : formatParticleNumber(spec.defaultValue), isDefault: true };
  if (spec.kind === "pair" && isNumberList(value) && value.length === 2) return { text: value.map(formatParticleNumber).join(` ${RANGE_DASH} `), isDefault: false };
  if (spec.kind === "points" && isNumberList(value)) return { text: value.map(formatParticleNumber).join(POINT_SEPARATOR), isDefault: false };
  return { text: textOfValue(value), isDefault: false };
}

/** Одно число: запятая и точка в дробной части, короткий и длинный минус; `null` — не число. */
function parseOneNumber(text: string): number | null {
  return parseNumberField(text.trim().replace(MINUS, "-"));
}

const NUMBER = String.raw`[-−]?(?:\d+(?:[.,]\d*)?|[.,]\d+)(?:[eE][-+]?\d+)?`;
// «от» и «до» разделяет тире, многоточие, пробел или дефис — вплотную («4-6») или с пробелами с обеих сторон («4 - 6»).
// Дефис, к которому вплотную прижато второе число после пробела, — его минус: «-20 -10» — это от −20 до −10.
const PAIR = new RegExp(String.raw`^\s*(${NUMBER})(?:\s*[–—]\s*|\s+-\s+|-|\s*\.\.\.?\s*|\s*…\s*|\s+)(${NUMBER})\s*$`);
// Точки видимости разделяет точка посередине, точка с запятой, пробел или запятая с пробелом: «0 · 0,7 · 0», «0 0,7 0», «0, 0.7, 0».
const POINT_SPLIT = /\s*[·;]\s*|,\s+|\s+/;

/**
 * Значение поля из набранного текста — «Редактор», требование 26. Пустое поле убирает ключ. Пара: «от – до», равные
 * пишутся одним числом, одно число — тоже числом. Видимость: одна точка пишется числом, несколько — рядом. Не число —
 * `isValid` ложно.
 */
export function parseParticleFieldText(spec: ParticleFieldSpec, text: string): ParsedParticleField {
  if (text.trim() === "") return { isValid: true, value: undefined };
  if (spec.kind === "pair") {
    const range = PAIR.exec(text);
    if (range !== null) {
      const from = parseOneNumber(range[1] as string);
      const to = parseOneNumber(range[2] as string);
      if (from === null || to === null) return { isValid: false };
      return { isValid: true, value: from === to ? from : [from, to] };
    }
  }
  if (spec.kind === "points") {
    const points = text.trim().split(POINT_SPLIT).map(parseOneNumber);
    if (points.includes(null)) return { isValid: false };
    return { isValid: true, value: points.length === 1 ? (points[0] as number) : (points as number[]) };
  }
  const number = parseOneNumber(text);
  return number === null ? { isValid: false } : { isValid: true, value: number };
}
