import type { SceneSize } from "./sceneObjects";

/** Уровень и цвет воды, как их пишет `water` в файле рельефа. */
export type TerrainWater = { level: number; color: string };

/** Сетка высот сцены строками сверху вниз: точка `(column, row)` лежит в месте сцены `(column / 2, row / 2)`. */
export type TerrainGrid = { columns: number; rows: number; heights: Float64Array };

/** Что лежит в файле рельефа: сетка высот и вода, если она есть. */
export type TerrainContent = TerrainGrid & { water: TerrainWater | null };

/** «Рельеф», требование 23: вода, которую включает галочка, — земля высоты 0 остаётся сушей. */
export const DEFAULT_WATER: TerrainWater = { level: -0.5, color: "#3f7fd0" };

const HUNDREDTHS = 100;

export function roundToHundredths(value: number): number {
  return Math.round(value * HUNDREDTHS) / HUNDREDTHS;
}

/** Число в файле: до сотых без лишних нулей, `-0` — `0`. */
function formatNumber(value: number): string {
  const rounded = roundToHundredths(value);
  return rounded === 0 ? "0" : String(rounded);
}

/** Перенос строки, каким уже набран текст, — файл, набранный в Windows, не превращается в смесь. */
function lineBreakOf(text: string | null): string {
  return text !== null && text.includes("\r\n") ? "\r\n" : "\n";
}

/**
 * Текст файла рельефа — «Кисти рельефа», требование 18: `water` первой строкой, если вода есть,
 * затем `heights` — по строке сетки на строку файла, числа через запятую с пробелом, как лежит
 * `games/rpg/terrain.json`.
 */
export function formatTerrainText(content: TerrainContent, lineBreak = "\n"): string {
  const lines: string[] = ["{"];
  if (content.water !== null) {
    lines.push(`  "water": { "level": ${formatNumber(content.water.level)}, "color": ${JSON.stringify(content.water.color)} },`);
  }
  lines.push('  "heights": [');
  for (let row = 0; row < content.rows; row += 1) {
    const numbers = Array.from(content.heights.subarray(row * content.columns, (row + 1) * content.columns), formatNumber);
    lines.push(`    [${numbers.join(", ")}]${row + 1 < content.rows ? "," : ""}`);
  }
  lines.push("  ]", "}");
  return lines.join(lineBreak) + lineBreak;
}

function isTerrainWater(value: unknown): value is TerrainWater {
  if (value === null || typeof value !== "object") return false;
  const water = value as { level?: unknown; color?: unknown };
  return typeof water.level === "number" && typeof water.color === "string";
}

/** Файл рельефа как сетка и вода; текст, что не разбирается, — `null` (проверять его — дело движка). */
export function parseTerrainText(text: string): TerrainContent | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const file = parsed as { heights?: unknown; water?: unknown } | null;
  const rows = file?.heights;
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every((row) => Array.isArray(row) && row.length === rows[0].length)) return null;
  if (!rows.every((row: unknown[]) => row.every((height) => typeof height === "number"))) return null;
  const water = isTerrainWater(file?.water) ? file.water : null;
  return { columns: rows[0].length, rows: rows.length, heights: Float64Array.from((rows as number[][]).flat()), water: water === null ? null : { level: water.level, color: water.color } };
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

/** Текст рельефа после мазка: высоты — из сетки, вода и перенос строки — как были. */
export function terrainTextWithHeights(previousText: string | null, grid: TerrainGrid): string {
  return formatTerrainText({ ...grid, water: readTerrainWater(previousText) }, lineBreakOf(previousText));
}

/**
 * Текст рельефа после правки воды: высоты — как в файле, без файла — ровная земля. Файл, что не
 * разбирается, — `null`: править нечего.
 */
export function terrainTextWithWater(previousText: string | null, sceneSize: SceneSize, water: TerrainWater | null): string | null {
  const previous = previousText === null ? { ...flatTerrainGrid(sceneSize), water: null } : parseTerrainText(previousText);
  return previous === null ? null : formatTerrainText({ ...previous, water }, lineBreakOf(previousText));
}

/** Хоть одна высота отличается после округления до сотых — иначе мазок не действие (требование 18). */
export function differsInHundredths(first: ArrayLike<number>, second: ArrayLike<number>): boolean {
  for (let index = 0; index < first.length; index += 1) {
    if (Math.round((first[index] as number) * HUNDREDTHS) !== Math.round((second[index] as number) * HUNDREDTHS)) return true;
  }
  return false;
}
