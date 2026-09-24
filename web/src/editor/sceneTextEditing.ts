import { applyEdits, modify, type JSONPath } from "jsonc-parser";

/**
 * Значение свойства или объекта в том виде, в каком его пишут вручную в демо-играх — пробел после
 * `{`, `:` и `,`, без переноса строки: `{ "position": [1, 6], "size": [1, 1], "image": "wall" }`,
 * `"hp": 3` — «Редактор», требование 5. `JSON.stringify` такой пробел не расставляет, а
 * `jsonc-parser` без `formattingOptions` вставляет значение вовсе без пробелов — эту раскладку
 * редактор собирает сам.
 */
export function formatSceneValue(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(formatSceneValue).join(", ")}]`;
  if (value !== null && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return "{}";
    return `{ ${entries.map(([key, entryValue]) => `${JSON.stringify(key)}: ${formatSceneValue(entryValue)}`).join(", ")} }`;
  }
  return JSON.stringify(value);
}

/**
 * `jsonc-parser` без `formattingOptions` вставляет новый текст как хвост своего же `content`
 * (`JSON.stringify(value)`) — при замене значения он и есть весь `content`, при новом свойстве или
 * элементе массива ему предшествует свой собственный кусок (`"ключ": `, запятая) — этот кусок
 * `jsonc-parser` строит сам и не трогается; переформатирован по требованию 5 только сам JSON
 * значения, в самом хвосте `content`.
 */
function reformatEditContent(content: string, value: unknown): string {
  const compact = JSON.stringify(value);
  let prefix = content.slice(0, content.length - compact.length);
  // Ведущая запятая перед новым свойством или элементом массива — без пробела у `jsonc-parser`,
  // а в демо-играх между значениями всегда пробел после запятой (требование 5).
  if (prefix.startsWith(",")) prefix = `, ${prefix.slice(1)}`;
  return prefix + formatSceneValue(value);
}

/**
 * Правка текста на месте через `jsonc-parser`, без `formattingOptions` — требование 5: заменяется
 * только своё значение или добавляется свой ключ/элемент, остальной текст, выравнивание и переносы
 * остаются байт в байт. Значение `undefined` — удаление. Пути, у которых нет узла (объект не
 * существует), дают исходный текст без изменений — вызывающая сторона сама решает, что это значит.
 */
function editAt(text: string, path: JSONPath, value: unknown, isArrayInsertion: boolean): string {
  const edits = modify(text, path, value, { isArrayInsertion });
  if (edits.length === 0) return text;
  const patchedEdits = value === undefined ? edits : edits.map((edit) => ({ ...edit, content: reformatEditContent(edit.content, value) }));
  return applyEdits(text, patchedEdits);
}

/** Заменяет значение свойства объекта `objects[objectIndex].<key>` — требования 8–9, 12–13. */
export function setObjectPropertyValue(sceneText: string, objectIndex: number, key: string, value: unknown): string {
  return editAt(sceneText, ["objects", objectIndex, key], value, false);
}

/** Удаляет свойство объекта — требование 14. Свойства нет — текст не меняется. */
export function removeObjectProperty(sceneText: string, objectIndex: number, key: string): string {
  return editAt(sceneText, ["objects", objectIndex, key], undefined, false);
}

/** Дописывает свойство в конец объекта — требование 15. Свойство уже есть — вызывающая сторона проверяет сама, до вызова. */
export function addObjectProperty(sceneText: string, objectIndex: number, key: string, value: unknown): string {
  return editAt(sceneText, ["objects", objectIndex, key], value, false);
}

/** Дописывает объект в конец `objects` — требования 17 (копия) и раздел «Технические детали». */
export function appendSceneObject(sceneText: string, objectCount: number, object: unknown): string {
  return editAt(sceneText, ["objects", objectCount], object, true);
}

/** Удаляет объект из `objects` — требование 18: номера следующих объектов сдвигаются сами, это одна правка индекса. */
export function removeSceneObject(sceneText: string, objectIndex: number): string {
  return editAt(sceneText, ["objects", objectIndex], undefined, false);
}

/** Объявляет новое свойство автора в `properties.json` — требование 16. */
export function declarePropertyKind(propertiesText: string, name: string, kind: string): string {
  return editAt(propertiesText, ["properties", name], kind, false);
}
