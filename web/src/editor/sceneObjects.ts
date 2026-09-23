export type SceneObjectSummary = {
  index: number;
  name: string | null;
  color: string | null;
  image: string | null;
  /** Без своих `position` и `size` объекта на сцене нет — он есть только в списке. */
  isOnScene: boolean;
};

/** Размер сцены в клетках — `scene.width` и `scene.height` из `game.json`. */
export type SceneSize = { width: number; height: number };

export type ObjectPropertiesView =
  | { status: "none" }
  | { status: "not-object"; json: string }
  | { status: "object"; properties: { key: string; valueText: string }[] };

/**
 * Список и свойства редактор берёт из текста `scene.json` сам, отдельно от разбора движком —
 * «Редактор», требование 27. Не прочитан, не разбирается как JSON или в нём нет массива
 * `objects` — список пуст (требование 28), а не ошибка разбора наружу.
 */
export function parseSceneObjects(sceneText: string | null): unknown[] {
  if (sceneText === null) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(sceneText);
  } catch {
    return [];
  }
  const objects = (parsed as { objects?: unknown } | null)?.objects;
  return Array.isArray(objects) ? objects : [];
}

const SCENE_COLOR_PATTERN = /^#[0-9a-fA-F]{6}$/;

/** Цвет в том единственном виде, который движок принимает в данных сцены, — `#rrggbb`. */
export function isSceneColor(value: string): boolean {
  return SCENE_COLOR_PATTERN.test(value);
}

function readStringField(entry: unknown, key: string): string | null {
  if (entry === null || typeof entry !== "object" || Array.isArray(entry)) return null;
  const value = (entry as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

function hasOwnField(entry: unknown, key: string): boolean {
  return entry !== null && typeof entry === "object" && !Array.isArray(entry) && key in entry;
}

/**
 * Строка списка — номер объекта по месту в файле и его `name`, если это строка («Редактор»,
 * требование 27); цвет, картинка и наличие на сцене — для значка строки.
 */
export function summarizeSceneObjects(objects: unknown[]): SceneObjectSummary[] {
  return objects.map((entry, index) => {
    const color = readStringField(entry, "color");
    return {
      index,
      name: readStringField(entry, "name"),
      color: color !== null && isSceneColor(color) ? color : null,
      image: readStringField(entry, "image"),
      isOnScene: hasOwnField(entry, "position") && hasOwnField(entry, "size"),
    };
  });
}

/**
 * Размер сцены в клетках из `game.json` — по нему редактор подгоняет холст под пропорции сцены.
 * Файл не разбирается или размера в нём нет — `null`, холст тогда занимает всю часть окна.
 */
export function parseSceneSize(gameJsonText: string | null): SceneSize | null {
  if (gameJsonText === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(gameJsonText);
  } catch {
    return null;
  }
  const scene = (parsed as { scene?: { width?: unknown; height?: unknown } } | null)?.scene;
  const width = scene?.width;
  const height = scene?.height;
  if (typeof width !== "number" || typeof height !== "number" || width <= 0 || height <= 0) return null;
  return { width, height };
}

/**
 * Свойства выбранного объекта — по строке на ключ, в порядке файла, значение компактным JSON
 * («Редактор», требования 29–30). Элемент `objects`, который не объект (например, строка или
 * `null`), — одной строкой его JSON, а не список свойств.
 */
export function buildObjectPropertiesView(objects: unknown[], selectedIndex: number | null): ObjectPropertiesView {
  if (selectedIndex === null) return { status: "none" };
  const entry = objects[selectedIndex];
  if (entry === undefined) return { status: "none" };
  if (entry === null || typeof entry !== "object" || Array.isArray(entry)) {
    return { status: "not-object", json: JSON.stringify(entry) };
  }
  const properties = Object.entries(entry as Record<string, unknown>).map(([key, value]) => ({
    key,
    valueText: JSON.stringify(value),
  }));
  return { status: "object", properties };
}

/**
 * После перезагрузки выбран объект с тем же номером, если он есть в новом `scene.json`; иначе
 * выбор снят («Редактор», требование 36).
 */
export function resolveSelectionAfterReload(selectedIndex: number | null, objectCount: number): number | null {
  if (selectedIndex === null) return null;
  return selectedIndex < objectCount ? selectedIndex : null;
}
