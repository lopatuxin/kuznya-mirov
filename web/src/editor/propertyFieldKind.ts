import { isSceneColor } from "./sceneObjects";

export type PropertyFieldKind =
  | { kind: "checkbox" }
  | { kind: "select"; options: readonly (string | number)[] }
  | { kind: "color" }
  | { kind: "text" };

const ROTATION_OPTIONS = [0, 90, 180, 270] as const;
const FOLLOW_MOUSE_OPTIONS = ["x", "y", "xy"] as const;

/**
 * Вид поля для значения свойства — «Редактор», требование 11: галочка — `collides` и объявленные
 * свойства автора вида `flag`; выпадающий список — `image` (картинки `files.images`), `rotation`
 * (0/90/180/270), `follow_mouse` (`x`/`y`/`xy`); `color` — палитра; остальное — текст. Значение,
 * которого нет в наборе поля (`rotation: 45`, `collides: 1`, картинки нет в списке), — текстовое поле.
 */
export function propertyFieldKind(
  key: string,
  value: unknown,
  authorPropertyKinds: Readonly<Record<string, string>>,
  imageNames: readonly string[],
): PropertyFieldKind {
  if (key === "collides" || authorPropertyKinds[key] === "flag") {
    return typeof value === "boolean" ? { kind: "checkbox" } : { kind: "text" };
  }
  if (key === "image") {
    return typeof value === "string" && imageNames.includes(value) ? { kind: "select", options: imageNames } : { kind: "text" };
  }
  if (key === "rotation") {
    return typeof value === "number" && (ROTATION_OPTIONS as readonly number[]).includes(value)
      ? { kind: "select", options: ROTATION_OPTIONS }
      : { kind: "text" };
  }
  if (key === "follow_mouse") {
    return typeof value === "string" && (FOLLOW_MOUSE_OPTIONS as readonly string[]).includes(value)
      ? { kind: "select", options: FOLLOW_MOUSE_OPTIONS }
      : { kind: "text" };
  }
  if (key === "color") {
    return typeof value === "string" && isSceneColor(value) ? { kind: "color" } : { kind: "text" };
  }
  return { kind: "text" };
}
