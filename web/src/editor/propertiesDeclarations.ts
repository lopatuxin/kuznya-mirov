import { PARTICLE_PROPERTY_NAMES } from "./particleEffects";

/** Виды свойств автора — «Формат игры»: `flag`, `number`, `time`, `timer`, `text`. */
export type PropertyKind = "flag" | "number" | "time" | "timer" | "text";

export const PROPERTY_KINDS: readonly PropertyKind[] = ["flag", "number", "time", "timer", "text"];

/**
 * Свойства движка — закрытый список («Формат игры» → «Объекты и свойства»). `name` в список не
 * входит: у объекта оно есть почти всегда, но подсказка для «+ свойство» по нему не нужна отдельно —
 * оно как раз обычно уже стоит.
 */
export const ENGINE_PROPERTY_NAMES: readonly string[] = [
  "position",
  "size",
  "velocity",
  "grid",
  "collides",
  "deck",
  "color",
  "layer",
  "image",
  "opacity",
  "rotation",
  "flip_x",
  "shape",
  "height",
  "keys",
  "follow_mouse",
  "lifetime",
  "name",
  "camera_follows",
  "walk_to",
  "walk_speed",
  "on_click",
  "parallax",
  "repeat_x",
  "sway",
  ...PARTICLE_PROPERTY_NAMES,
];

/** Свойства движка, которые есть только в плоской сцене: в трёхмерной «+ свойство» их не предлагает. */
const FLAT_SCENE_ONLY_PROPERTY_NAMES: readonly string[] = ["sway", ...PARTICLE_PROPERTY_NAMES];

/**
 * Свойства автора из `properties.json` — имя → вид. Не читается или не разбирается — пустой список:
 * «+ свойство» тогда не предложит объявленные свойства и не даст объявить новое (крайние случаи).
 */
export function parsePropertyDeclarations(propertiesText: string | null): Record<string, PropertyKind> {
  if (propertiesText === null) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(propertiesText);
  } catch {
    return {};
  }
  const properties = (parsed as { properties?: unknown } | null)?.properties;
  if (properties === null || typeof properties !== "object" || Array.isArray(properties)) return {};
  const result: Record<string, PropertyKind> = {};
  for (const [name, kind] of Object.entries(properties as Record<string, unknown>)) {
    if (typeof kind === "string" && (PROPERTY_KINDS as readonly string[]).includes(kind)) result[name] = kind as PropertyKind;
  }
  return result;
}

/**
 * Подсказки для «+ свойство» — «Редактор», требование 15: свойства движка и объявленные свойства
 * автора, которых у объекта ещё нет, в исходном порядке списков; `sway` и свойства частиц — только в плоской сцене.
 */
export function suggestPropertyNames(
  existingKeys: readonly string[],
  declaredProperties: Readonly<Record<string, PropertyKind>>,
  isThreeDimensionalScene: boolean,
): string[] {
  const existing = new Set(existingKeys);
  const engineNames = ENGINE_PROPERTY_NAMES.filter((name) => !existing.has(name) && !(isThreeDimensionalScene && FLAT_SCENE_ONLY_PROPERTY_NAMES.includes(name)));
  const authorNames = Object.keys(declaredProperties).filter((name) => !existing.has(name));
  return [...engineNames, ...authorNames];
}

/**
 * Пустое значение по умолчанию для нового объявленного свойства — требование 16: флаг — `true`,
 * строка — пустая строка («Таблицы данных», требование 39: пустое значение для `text` допустимо),
 * остальное решает вызывающая сторона.
 */
export function defaultValueForPropertyKind(kind: PropertyKind): unknown {
  if (kind === "flag") return true;
  if (kind === "text") return "";
  return undefined;
}
