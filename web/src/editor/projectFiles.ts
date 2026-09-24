export type ProjectFilePaths = { scene: string; properties: string };

/**
 * Пути `scene.json` и `properties.json` из `files.scene`/`files.properties` в `game.json` —
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
  const files = (parsed as { files?: { scene?: unknown; properties?: unknown } } | null)?.files;
  const scene = files?.scene;
  const properties = files?.properties;
  if (typeof scene !== "string" || typeof properties !== "string") return null;
  return { scene, properties };
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
