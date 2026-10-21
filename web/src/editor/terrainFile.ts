import type { SceneSize } from "./sceneObjects";

/** Озеро из `lakes` файла рельефа: точка в чаше и высота глади. */
export type WaterLake = { at: [number, number]; level: number };

/** Река из `rivers` файла рельефа: точки `[x, y, h, w]` от истока к устью. */
export type WaterRiver = { points: [number, number, number, number][] };

/**
 * Вода, как её пишет `water` в файле рельефа — «Рельеф», «Вода»: цвет, море `level`, озёра, реки, прозрачность и рябь.
 * Поля, которых в файле нет, не заполняются: `clarity` и `ripple` пишутся, только когда автор их правил.
 */
export type TerrainWater = { color: string; level?: number; lakes?: WaterLake[]; rivers?: WaterRiver[]; clarity?: number; ripple?: number };

/** Сетка высот сцены строками сверху вниз: точки через равные доли клетки, по две — четыре на клетку. */
export type TerrainGrid = { columns: number; rows: number; heights: Float64Array };

/** Слой покрытия из `covers` файла рельефа: материал, порог крутизны и маска; у первого слоя ни порога, ни маски нет. */
export type TerrainCoverLayer = { material: string; slope?: number; mask?: string };

/**
 * Отпечаток из `stamps` файла рельефа, как он там лежит: `{stamp, position, size, height, rotation?}`.
 * Значения не проверяются — набранное в свойствах пишется как есть, ошибку называет движок.
 */
export type ImprintEntry = Record<string, unknown>;

/** Что лежит в файле рельефа: сетка высот, вода, покрытия, путь карты цвета `tint` и отпечатки `stamps`, если они есть. Карту цвета редактор не правит, только переносит. */
export type TerrainContent = TerrainGrid & {
  water: TerrainWater | null;
  covers: TerrainCoverLayer[] | null;
  tint?: string | null;
  stamps?: ImprintEntry[] | null;
};

/** «Рельеф», требование 23: вода, которую включает галочка, — земля высоты 0 остаётся сушей. */
export const DEFAULT_WATER = { level: -0.5, color: "#3f7fd0" } satisfies TerrainWater;

const HUNDREDTHS = 100;

export function roundToHundredths(value: number): number {
  return Math.round(value * HUNDREDTHS) / HUNDREDTHS;
}

/** Море в воде; воды нет — моря нет. */
export function hasSea(water: TerrainWater | null): boolean {
  return water?.level !== undefined;
}

/** Галочка «Море»: без воды включает море на уровне по умолчанию, со снятой — возвращает его; снятая убирает только `level`, а вода без моря, озёр и рек уходит целиком. */
export function waterWithSea(water: TerrainWater | null, isSea: boolean): TerrainWater | null {
  if (isSea) return water === null ? DEFAULT_WATER : { ...water, level: water.level ?? DEFAULT_WATER.level };
  if (water === null) return null;
  const withoutSea = { ...water };
  delete withoutSea.level;
  return (withoutSea.lakes?.length ?? 0) > 0 || (withoutSea.rivers?.length ?? 0) > 0 ? withoutSea : null;
}

/** Прозрачность или рябь после правки автора; `null` убирает ключ из файла — значение по умолчанию. */
export function waterWithOption(water: TerrainWater, key: "clarity" | "ripple", value: number | null): TerrainWater {
  const changed = { ...water };
  if (value === null) delete changed[key];
  else changed[key] = value;
  return changed;
}

/** Число в файле: до сотых без лишних нулей, `-0` — `0`. */
function formatNumber(value: number): string {
  const rounded = roundToHundredths(value);
  return rounded === 0 ? "0" : String(rounded);
}

const IMPRINT_KEYS = ["stamp", "position", "size", "height", "rotation"];

function formatImprintValue(value: unknown): string {
  if (typeof value === "number" && Number.isFinite(value)) return formatNumber(value);
  if (Array.isArray(value) && value.every((item) => typeof item === "number" && Number.isFinite(item))) return `[${value.map(formatNumber).join(", ")}]`;
  return JSON.stringify(value);
}

/** Отпечаток одной строкой: ключи в порядке `stamp`, `position`, `size`, `height`, `rotation`, числа до сотых; `rotation` пишется, если не 0. */
function formatImprint(imprint: ImprintEntry): string {
  const keys = [...IMPRINT_KEYS, ...Object.keys(imprint).filter((key) => !IMPRINT_KEYS.includes(key))];
  const fields = keys
    .filter((key) => key in imprint && !(key === "rotation" && formatImprintValue(imprint[key]) === "0"))
    .map((key) => `${JSON.stringify(key)}: ${formatImprintValue(imprint[key])}`);
  return `{ ${fields.join(", ")} }`;
}

/** Перенос строки, каким уже набран текст, — файл, набранный в Windows, не превращается в смесь. */
function lineBreakOf(text: string | null): string {
  return text !== null && text.includes("\r\n") ? "\r\n" : "\n";
}

/** Число воды в файле как есть: озёра, реки, прозрачность и рябь редактор не округляет, `-0` — `0`. */
function formatWaterNumber(value: number): string {
  return String(value === 0 ? 0 : value);
}

function formatWaterNumbers(values: readonly number[]): string {
  return `[${values.map(formatWaterNumber).join(", ")}]`;
}

/** Блок `water` в начале файла: без озёр и рек — строкой, как всегда, с ними — шапкой и по водоёму на строку. */
function formatWater(water: TerrainWater): string[] {
  const { level, lakes = [], rivers = [], clarity, ripple } = water;
  const colorFields = [`"color": ${JSON.stringify(water.color)}`];
  if (clarity !== undefined) colorFields.push(`"clarity": ${formatWaterNumber(clarity)}`);
  if (ripple !== undefined) colorFields.push(`"ripple": ${formatWaterNumber(ripple)}`);
  if (lakes.length === 0 && rivers.length === 0) {
    const seaField = level === undefined ? [] : [`"level": ${formatNumber(level)}`];
    return [`  "water": { ${[...seaField, ...colorFields].join(", ")} },`];
  }
  if (level !== undefined) colorFields.push(`"level": ${formatNumber(level)}`);
  const lines = ['  "water": {', `    ${colorFields.join(", ")},`];
  const lakeLines = lakes.map(({ at, level: lakeLevel }) => `{ "at": ${formatWaterNumbers(at)}, "level": ${formatWaterNumber(lakeLevel)} }`);
  const riverLines = rivers.map(({ points }) => `{ "points": [${points.map(formatWaterNumbers).join(", ")}] }`);
  const blocks = [
    ...(lakeLines.length > 0 ? [{ key: "lakes", entries: lakeLines }] : []),
    ...(riverLines.length > 0 ? [{ key: "rivers", entries: riverLines }] : []),
  ];
  blocks.forEach(({ key, entries }, index) => {
    lines.push(`    "${key}": [`);
    entries.forEach((entry, entryIndex) => lines.push(`      ${entry}${entryIndex + 1 < entries.length ? "," : ""}`));
    lines.push(`    ]${index + 1 < blocks.length ? "," : ""}`);
  });
  lines.push("  },");
  return lines;
}

/**
 * Текст файла рельефа — «Кисти рельефа», требование 18, «Свет и материалы», требование 27, и «Лепка
 * рельефа», требование 33: `water` первой строкой, если вода есть, затем `covers` — по слою на строку,
 * если они есть, затем путь карты цвета `tint`, если он есть, затем `stamps` — по отпечатку на строку, затем `heights` — по строке сетки на строку файла, числа через запятую с пробелом, как лежит `games/rpg/terrain.json`.
 */
export function formatTerrainText(content: TerrainContent, lineBreak = "\n"): string {
  const lines: string[] = ["{"];
  if (content.water !== null) lines.push(...formatWater(content.water));
  const { covers } = content;
  if (covers !== null) {
    lines.push('  "covers": [');
    covers.forEach(({ material, slope, mask }, index) => {
      const fields = [`"material": ${JSON.stringify(material)}`];
      if (slope !== undefined) fields.push(`"slope": ${JSON.stringify(slope)}`);
      if (mask !== undefined) fields.push(`"mask": ${JSON.stringify(mask)}`);
      lines.push(`    { ${fields.join(", ")} }${index + 1 < covers.length ? "," : ""}`);
    });
    lines.push("  ],");
  }
  if (typeof content.tint === "string") lines.push(`  "tint": ${JSON.stringify(content.tint)},`);
  const { stamps } = content;
  if (stamps !== undefined && stamps !== null && stamps.length > 0) {
    lines.push('  "stamps": [');
    stamps.forEach((imprint, index) => lines.push(`    ${formatImprint(imprint)}${index + 1 < stamps.length ? "," : ""}`));
    lines.push("  ],");
  }
  lines.push('  "heights": [');
  for (let row = 0; row < content.rows; row += 1) {
    const numbers = Array.from(content.heights.subarray(row * content.columns, (row + 1) * content.columns), formatNumber);
    lines.push(`    [${numbers.join(", ")}]${row + 1 < content.rows ? "," : ""}`);
  }
  lines.push("  ]", "}");
  return lines.join(lineBreak) + lineBreak;
}

function isFiniteNumbers(value: unknown, count: number): value is number[] {
  return Array.isArray(value) && value.length === count && value.every((item) => typeof item === "number" && Number.isFinite(item));
}

/** Озёра и реки как они лежат в файле; не список — их нет, запись не нужной формы пропускается: проверять её — дело движка. */
function parseWaterLakes(value: unknown): WaterLake[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item: { at?: unknown; level?: unknown } | null) =>
    item !== null && typeof item === "object" && isFiniteNumbers(item.at, 2) && typeof item.level === "number" ? [{ at: [item.at[0], item.at[1]] as [number, number], level: item.level }] : [],
  );
}

function parseWaterRivers(value: unknown): WaterRiver[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item: { points?: unknown } | null) => {
    const points = item !== null && typeof item === "object" ? item.points : undefined;
    return Array.isArray(points) && points.every((point) => isFiniteNumbers(point, 4)) ? [{ points: points.map((point: number[]) => point.slice() as [number, number, number, number]) }] : [];
  });
}

/** Вода из файла: нужен цвет и хоть одно из моря, озёр и рек; иначе воды нет, как и раньше. */
function parseWater(value: unknown): TerrainWater | null {
  if (value === null || typeof value !== "object") return null;
  const water = value as { color?: unknown; level?: unknown; lakes?: unknown; rivers?: unknown; clarity?: unknown; ripple?: unknown };
  if (typeof water.color !== "string") return null;
  const lakes = parseWaterLakes(water.lakes);
  const rivers = parseWaterRivers(water.rivers);
  const hasLevel = typeof water.level === "number";
  if (!hasLevel && lakes.length === 0 && rivers.length === 0) return null;
  return {
    color: water.color,
    ...(hasLevel ? { level: water.level as number } : {}),
    ...(lakes.length > 0 ? { lakes } : {}),
    ...(rivers.length > 0 ? { rivers } : {}),
    ...(typeof water.clarity === "number" ? { clarity: water.clarity } : {}),
    ...(typeof water.ripple === "number" ? { ripple: water.ripple } : {}),
  };
}

function isCoverLayer(value: unknown): value is TerrainCoverLayer {
  if (value === null || typeof value !== "object") return false;
  const layer = value as { material?: unknown; slope?: unknown; mask?: unknown };
  return (
    typeof layer.material === "string" &&
    (layer.slope === undefined || typeof layer.slope === "number") &&
    (layer.mask === undefined || typeof layer.mask === "string")
  );
}

/** Слои `covers` как они лежат в файле; не список — `null`, слой не из `material` и `mask` пропускается: проверять их — дело движка. */
function parseCovers(value: unknown): TerrainCoverLayer[] | null {
  if (!Array.isArray(value)) return null;
  return value.filter(isCoverLayer).map(({ material, slope, mask }) => ({
    material,
    ...(slope === undefined ? {} : { slope }),
    ...(mask === undefined ? {} : { mask }),
  }));
}

/** Отпечатки `stamps` как они лежат в файле; не список — `null`, элемент, что не объект, пропускается: проверять их — дело движка. */
function parseImprints(value: unknown): ImprintEntry[] | null {
  if (!Array.isArray(value)) return null;
  return value.filter((item): item is ImprintEntry => item !== null && typeof item === "object" && !Array.isArray(item));
}

/** Файл рельефа как сетка, вода, покрытия и отпечатки; текст, что не разбирается, — `null` (проверять его — дело движка). */
export function parseTerrainText(text: string): TerrainContent | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const file = parsed as { heights?: unknown; water?: unknown; covers?: unknown; tint?: unknown; stamps?: unknown } | null;
  const rows = file?.heights;
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every((row) => Array.isArray(row) && row.length === rows[0].length)) return null;
  if (!rows.every((row: unknown[]) => row.every((height) => typeof height === "number"))) return null;
  return {
    columns: rows[0].length,
    rows: rows.length,
    heights: Float64Array.from((rows as number[][]).flat()),
    water: parseWater(file?.water),
    covers: parseCovers(file?.covers),
    tint: typeof file?.tint === "string" ? file.tint : null,
    stamps: parseImprints(file?.stamps),
  };
}

/** Отпечатки из файла рельефа в порядке файла; файла нет или он не разбирается — отпечатков нет. */
export function readTerrainImprints(text: string | null): ImprintEntry[] {
  return text === null ? [] : (parseTerrainText(text)?.stamps ?? []);
}

/** Слои покрытий из файла рельефа: файла нет, он не разбирается или в нём нет `covers` — `null`. */
export function readTerrainCovers(text: string | null): TerrainCoverLayer[] | null {
  return text === null ? null : (parseTerrainText(text)?.covers ?? null);
}

/** Путь карты цвета `tint` из файла рельефа: файла нет, он не разбирается или в нём нет `tint` — `null`. */
export function readTerrainTint(text: string | null): string | null {
  return text === null ? null : (parseTerrainText(text)?.tint ?? null);
}

/** Вода из файла рельефа: файла нет или в нём нет `water` — `null`. */
export function readTerrainWater(text: string | null): TerrainWater | null {
  return text === null ? null : (parseTerrainText(text)?.water ?? null);
}

/** Ровная земля высоты 0 нужного размера: `(2 × высота + 1) × (2 × ширина + 1)` чисел. */
export function flatTerrainGrid(sceneSize: SceneSize): TerrainGrid {
  const columns = 2 * sceneSize.width + 1;
  const rows = 2 * sceneSize.height + 1;
  return { columns, rows, heights: new Float64Array(columns * rows) };
}

/** Текст рельефа после мазка: высоты — из сетки, вода, покрытия, отпечатки и перенос строки — как были. */
export function terrainTextWithHeights(previousText: string | null, grid: TerrainGrid): string {
  const previous = previousText === null ? null : parseTerrainText(previousText);
  return formatTerrainText(
    { ...grid, water: previous?.water ?? null, covers: previous?.covers ?? null, tint: previous?.tint ?? null, stamps: previous?.stamps ?? null },
    lineBreakOf(previousText),
  );
}

/** Прежний файл рельефа или, без него, ровная земля сцены без воды и покрытий; файл, что не разбирается, — `null`. */
function previousContentOrFlat(previousText: string | null, sceneSize: SceneSize): TerrainContent | null {
  return previousText === null ? { ...flatTerrainGrid(sceneSize), water: null, covers: null } : parseTerrainText(previousText);
}

/**
 * Текст рельефа после правки воды: высоты и покрытия — как в файле, без файла — ровная земля без
 * покрытий. Файл, что не разбирается, — `null`: править нечего.
 */
export function terrainTextWithWater(previousText: string | null, sceneSize: SceneSize, water: TerrainWater | null): string | null {
  const previous = previousContentOrFlat(previousText, sceneSize);
  return previous === null ? null : formatTerrainText({ ...previous, water }, lineBreakOf(previousText));
}

/**
 * Текст рельефа после правки отпечатков — «Лепка рельефа», требование 33: высоты, вода, покрытия и карта цвета
 * — как в файле, без файла — ровная земля без воды и покрытий; пустой список убирает ключ `stamps`.
 * Файл, что не разбирается, — `null`: править нечего.
 */
export function terrainTextWithImprints(previousText: string | null, sceneSize: SceneSize, imprints: readonly ImprintEntry[]): string | null {
  const previous = previousContentOrFlat(previousText, sceneSize);
  return previous === null ? null : formatTerrainText({ ...previous, stamps: imprints.length === 0 ? null : [...imprints] }, lineBreakOf(previousText));
}

/**
 * Текст рельефа после покраски — «Покраска», требования 12–13, 15: слои покрытий новые, высоты, вода, отпечатки и
 * карта цвета — как в файле, без файла — ровная земля без воды. Файл, что не разбирается, — `null`: править нечего.
 */
export function terrainTextWithCovers(previousText: string | null, sceneSize: SceneSize, covers: readonly TerrainCoverLayer[]): string | null {
  const previous = previousContentOrFlat(previousText, sceneSize);
  return previous === null ? null : formatTerrainText({ ...previous, covers: [...covers] }, lineBreakOf(previousText));
}

/** Хоть одна высота отличается после округления до сотых — иначе мазок не действие (требование 18). */
export function differsInHundredths(first: ArrayLike<number>, second: ArrayLike<number>): boolean {
  for (let index = 0; index < first.length; index += 1) {
    if (Math.round((first[index] as number) * HUNDREDTHS) !== Math.round((second[index] as number) * HUNDREDTHS)) return true;
  }
  return false;
}
