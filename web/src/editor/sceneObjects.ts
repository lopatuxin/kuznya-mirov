export type SceneObjectSummary = { index: number; name: string | null };

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

/**
 * Строка списка — номер объекта по месту в файле и его `name`, если это строка («Редактор»,
 * требование 27).
 */
export function summarizeSceneObjects(objects: unknown[]): SceneObjectSummary[] {
  return objects.map((entry, index) => {
    const name =
      entry !== null && typeof entry === "object" && "name" in entry && typeof (entry as { name?: unknown }).name === "string"
        ? ((entry as { name: string }).name)
        : null;
    return { index, name };
  });
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
