/** Виды свойств автора — «Формат игры»: `flag`, `number`, `time`, `timer`. */
export type PropertyKind = "flag" | "number" | "time" | "timer";

export const PROPERTY_KINDS: readonly PropertyKind[] = ["flag", "number", "time", "timer"];

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
  "color",
  "layer",
  "image",
  "opacity",
  "rotation",
  "keys",
  "follow_mouse",
  "lifetime",
  "name",
];

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
 * автора, которых у объекта ещё нет, в исходном порядке списков.
 */
export function suggestPropertyNames(existingKeys: readonly string[], declaredProperties: Readonly<Record<string, PropertyKind>>): string[] {
  const existing = new Set(existingKeys);
  const engineNames = ENGINE_PROPERTY_NAMES.filter((name) => !existing.has(name));
  const authorNames = Object.keys(declaredProperties).filter((name) => !existing.has(name));
  return [...engineNames, ...authorNames];
}

/** Пустое значение по умолчанию для нового объявленного свойства — требование 16: флаг — `true`, остальное решает вызывающая сторона. */
export function defaultValueForPropertyKind(kind: PropertyKind): unknown {
  return kind === "flag" ? true : undefined;
}
