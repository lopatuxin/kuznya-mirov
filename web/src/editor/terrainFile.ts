import type { SceneSize } from "./sceneObjects";

/** Уровень и цвет воды, как их пишет `water` в файле рельефа. */
export type TerrainWater = { level: number; color: string };

/** Сетка высот сцены строками сверху вниз: точки через равные доли клетки, по две — четыре на клетку. */
export type TerrainGrid = { columns: number; rows: number; heights: Float64Array };

/** Слой покрытия из `covers` файла рельефа: материал и маска, у первого слоя маски нет. */
export type TerrainCoverLayer = { material: string; mask?: string };

/** Что лежит в файле рельефа: сетка высот, вода, покрытия и путь карты цвета `tint`, если они есть. Покрытия и карту цвета редактор не правит, только переносит. */
export type TerrainContent = TerrainGrid & { water: TerrainWater | null; covers: TerrainCoverLayer[] | null; tint?: string | null };

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
 * Текст файла рельефа — «Кисти рельефа», требование 18, и «Свет и материалы», требование 27: `water`
 * первой строкой, если вода есть, затем `covers` — по слою на строку, если они есть, затем путь карты
 * цвета `tint`, если он есть, затем `heights` — по строке сетки на строку файла, числа через запятую с пробелом, как лежит `games/rpg/terrain.json`.
 * Тот же вид пишет построитель локации (`tools/location/files.mjs`).
 */
export function formatTerrainText(content: TerrainContent, lineBreak = "\n"): string {
  const lines: string[] = ["{"];
  if (content.water !== null) {
    lines.push(`  "water": { "level": ${formatNumber(content.water.level)}, "color": ${JSON.stringify(content.water.color)} },`);
  }
  const { covers } = content;
  if (covers !== null) {
    lines.push('  "covers": [');
    covers.forEach(({ material, mask }, index) => {
      const fields = [`"material": ${JSON.stringify(material)}`];
      if (mask !== undefined) fields.push(`"mask": ${JSON.stringify(mask)}`);
      lines.push(`    { ${fields.join(", ")} }${index + 1 < covers.length ? "," : ""}`);
    });
    lines.push("  ],");
  }
  if (typeof content.tint === "string") lines.push(`  "tint": ${JSON.stringify(content.tint)},`);
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

function isCoverLayer(value: unknown): value is TerrainCoverLayer {
  if (value === null || typeof value !== "object") return false;
  const layer = value as { material?: unknown; mask?: unknown };
  return typeof layer.material === "string" && (layer.mask === undefined || typeof layer.mask === "string");
}

/** Слои `covers` как они лежат в файле; не список — `null`, слой не из `material` и `mask` пропускается: проверять их — дело движка. */
function parseCovers(value: unknown): TerrainCoverLayer[] | null {
  if (!Array.isArray(value)) return null;
  return value.filter(isCoverLayer).map(({ material, mask }) => (mask === undefined ? { material } : { material, mask }));
}

/** Файл рельефа как сетка, вода и покрытия; текст, что не разбирается, — `null` (проверять его — дело движка). */
export function parseTerrainText(text: string): TerrainContent | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const file = parsed as { heights?: unknown; water?: unknown; covers?: unknown; tint?: unknown } | null;
  const rows = file?.heights;
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every((row) => Array.isArray(row) && row.length === rows[0].length)) return null;
  if (!rows.every((row: unknown[]) => row.every((height) => typeof height === "number"))) return null;
  const water = isTerrainWater(file?.water) ? file.water : null;
  return {
    columns: rows[0].length,
    rows: rows.length,
    heights: Float64Array.from((rows as number[][]).flat()),
    water: water === null ? null : { level: water.level, color: water.color },
    covers: parseCovers(file?.covers),
    tint: typeof file?.tint === "string" ? file.tint : null,
  };
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

/** Текст рельефа после мазка: высоты — из сетки, вода, покрытия и перенос строки — как были. */
export function terrainTextWithHeights(previousText: string | null, grid: TerrainGrid): string {
  const previous = previousText === null ? null : parseTerrainText(previousText);
  return formatTerrainText({ ...grid, water: previous?.water ?? null, covers: previous?.covers ?? null, tint: previous?.tint ?? null }, lineBreakOf(previousText));
}

/**
 * Текст рельефа после правки воды: высоты и покрытия — как в файле, без файла — ровная земля без
 * покрытий. Файл, что не разбирается, — `null`: править нечего.
 */
export function terrainTextWithWater(previousText: string | null, sceneSize: SceneSize, water: TerrainWater | null): string | null {
  const previous = previousText === null ? { ...flatTerrainGrid(sceneSize), water: null, covers: null } : parseTerrainText(previousText);
  return previous === null ? null : formatTerrainText({ ...previous, water }, lineBreakOf(previousText));
}

/** Хоть одна высота отличается после округления до сотых — иначе мазок не действие (требование 18). */
export function differsInHundredths(first: ArrayLike<number>, second: ArrayLike<number>): boolean {
  for (let index = 0; index < first.length; index += 1) {
    if (Math.round((first[index] as number) * HUNDREDTHS) !== Math.round((second[index] as number) * HUNDREDTHS)) return true;
  }
  return false;
}
