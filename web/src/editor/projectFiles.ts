/** `terrain` — путь из `files.terrain`; `null`, пока у проекта нет файла рельефа («Кисти рельефа», требование 19). */
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
  return { scene, properties, terrain: typeof files?.terrain === "string" ? files.terrain : null };
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
