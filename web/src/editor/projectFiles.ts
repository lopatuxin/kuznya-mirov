import { isVideoPath } from "../images/imagePayload";

/**
 * `terrain` — путь из `files.terrain`; `null`, пока у проекта нет файла рельефа («Кисти рельефа», требование 19).
 */
export type ProjectFilePaths = { scene: string; properties: string; terrain: string | null };

/**
 * Пути `scene.json`, `properties.json` и рельефа из `files.scene`/`files.properties`/`files.terrain` в `game.json` —
 * «Редактор», требование 1: правка идёт по этим путям, какими бы их ни назвал автор игры. Не
 * читается, не разбирается или путей нет — `null`; движок в этом случае и так не даёт сцены, а
 * правка объектов недоступна (панель только показывает, крайние случаи).
 */
export function parseProjectFilePaths(gameJsonText: string | null): ProjectFilePaths | null {
  if (gameJsonText === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(gameJsonText);
  } catch {
    return null;
  }
  const files = (parsed as { files?: { scene?: unknown; properties?: unknown; terrain?: unknown } } | null)?.files;
  const scene = files?.scene;
  const properties = files?.properties;
  if (typeof scene !== "string" || typeof properties !== "string") return null;
  return {
    scene,
    properties,
    terrain: typeof files?.terrain === "string" ? files.terrain : null,
  };
}

/** Имена картинок из `files.images` — «Редактор», требование 11: набор выпадающего списка `image`. */
export function parseProjectImageNames(gameJsonText: string | null): string[] {
  if (gameJsonText === null) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(gameJsonText);
  } catch {
    return [];
  }
  const images = (parsed as { files?: { images?: unknown } } | null)?.files?.images;
  if (images === null || typeof images !== "object" || Array.isArray(images)) return [];
  return Object.keys(images as Record<string, unknown>);
}

/**
 * Куда лечь новому файлу рельефа — «Кисти рельефа», требование 19: в папку `scene.json`, под именем
 * `terrain.json`, а занятое имя — `terrain-2.json`, `terrain-3.json` и так далее, чтобы не затереть чужой файл.
 */
export async function chooseTerrainFilePath(scenePath: string, isTaken: (path: string) => Promise<boolean>): Promise<string> {
  const folder = scenePath.slice(0, scenePath.lastIndexOf("/") + 1);
  for (let attempt = 1; ; attempt += 1) {
    const path = `${folder}${attempt === 1 ? "terrain.json" : `terrain-${attempt}.json`}`;
    if (!(await isTaken(path))) return path;
  }
}

function readFilesSection(gameJsonText: string | null): Record<string, unknown> | null {
  if (gameJsonText === null) return null;
  try {
    const files = (JSON.parse(gameJsonText) as { files?: unknown } | null)?.files;
    return files !== null && typeof files === "object" && !Array.isArray(files) ? (files as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/** Материалы из `files.materials` в порядке объявления — «Покраска», требование 2: набор выпадающего списка «Материал». */
export function parseProjectMaterialNames(gameJsonText: string | null): string[] {
  const materials = readFilesSection(gameJsonText)?.materials;
  return materials !== null && typeof materials === "object" && !Array.isArray(materials) ? Object.keys(materials) : [];
}

/** Штампы из `files.stamps`: имя и путь файла — команде мазков их тексты нужны движку для итоговых высот. */
export function parseProjectStamps(gameJsonText: string | null): { name: string; path: string }[] {
  const stamps = readFilesSection(gameJsonText)?.stamps;
  if (stamps === null || typeof stamps !== "object" || Array.isArray(stamps)) return [];
  return Object.entries(stamps).flatMap(([name, path]) => (typeof path === "string" ? [{ name, path }] : []));
}

/** Описание картинки из `files.images`: что нужно редактору, чтобы показать её кадр и посчитать размер нового объекта. */
export type ProjectImageDescription = {
  name: string;
  /** Сколько кадров в файле; `null` — один кадр, весь файл. */
  frames: number | null;
  /** Сколько кадров в строке сетки; `null` — кадры лежат в одну строку. */
  columns: number | null;
  /** Свой размер картинки `size` в клетках; `null` — не задан. */
  size: readonly [number, number] | null;
  smooth: boolean;
};

function readPositiveNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;
}

/** Картинки `files.images` в порядке объявления — «Редактор», «Окно редактора», требование 24; в описании, которое не объект, берутся только имя и умолчания. */
export function parseProjectImageDescriptions(gameJsonText: string | null): ProjectImageDescription[] {
  const images = readFilesSection(gameJsonText)?.images;
  if (images === null || typeof images !== "object" || Array.isArray(images)) return [];
  return Object.entries(images).map(([name, description]) => {
    const fields = description !== null && typeof description === "object" && !Array.isArray(description) ? (description as Record<string, unknown>) : {};
    const size = Array.isArray(fields.size) ? [readPositiveNumber(fields.size[0]), readPositiveNumber(fields.size[1])] : null;
    return {
      name,
      frames: readPositiveNumber(fields.frames),
      columns: readPositiveNumber(fields.columns),
      size: size !== null && size[0] !== null && size[1] !== null ? [size[0], size[1]] : null,
      smooth: fields.smooth === true,
    };
  });
}

/** `scene.cell_pixels` из `game.json` — «Формат игры», требование 31: сколько точек картинки в клетке сцены; нет или не число больше нуля — `null`. */
export function parseProjectCellPixels(gameJsonText: string | null): number | null {
  if (gameJsonText === null) return null;
  try {
    const scene = (JSON.parse(gameJsonText) as { scene?: unknown } | null)?.scene;
    return scene !== null && typeof scene === "object" ? readPositiveNumber((scene as { cell_pixels?: unknown }).cell_pixels) : null;
  } catch {
    return null;
  }
}

function isCloudImageDescription(description: unknown, cellPixels: number | null): boolean {
  const fields = description !== null && typeof description === "object" && !Array.isArray(description) ? (description as Record<string, unknown>) : {};
  if (typeof fields.path === "string" && isVideoPath(fields.path)) return false;
  if (fields.frame_time !== undefined || fields.frame_by !== undefined || (readPositiveNumber(fields.frames) ?? 1) > 1) return false;
  if ((fields.anchor !== undefined && fields.anchor !== "center") || (fields.offset !== undefined && !(Array.isArray(fields.offset) && fields.offset.every((part) => part === 0)))) return false;
  const hasSize = Array.isArray(fields.size) && readPositiveNumber(fields.size[0]) !== null && readPositiveNumber(fields.size[1]) !== null;
  return hasSize || cellPixels !== null;
}

/**
 * Имена картинок, годных облакам, в порядке объявления — «Ветер и частицы», «Проверка перед запуском», требование 27: не видео, без кадров
 * (`frame_time`, `frame_by`, больше одного кадра), без `anchor` и `offset` и со своим `size` или при `cell_pixels` в игре.
 */
export function parseProjectCloudImageNames(gameJsonText: string | null): string[] {
  const images = readFilesSection(gameJsonText)?.images;
  if (images === null || typeof images !== "object" || Array.isArray(images)) return [];
  const cellPixels = parseProjectCellPixels(gameJsonText);
  return Object.entries(images).flatMap(([name, description]) => (isCloudImageDescription(description, cellPixels) ? [name] : []));
}
