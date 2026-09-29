import { isSceneColor } from "./sceneObjects";

export type PropertyFieldKind =
  | { kind: "checkbox" }
  | { kind: "select"; options: readonly (string | number)[] }
  | { kind: "color" }
  | { kind: "text" }
  /** Свойство автора вида `text` — набранное остаётся строкой как есть, не разбирается как JSON («Таблицы данных», требование 40). */
  | { kind: "raw-text" };

const ROTATION_OPTIONS = [0, 90, 180, 270] as const;
const SHAPE_OPTIONS = ["box", "cylinder", "capsule", "sphere"] as const;
const FOLLOW_MOUSE_OPTIONS = ["x", "y", "xy"] as const;

/**
 * Вид поля для значения свойства — «Редактор», требование 11: галочка — `collides`, `camera_follows`,
 * `flip_x` (Фаза 14, требование 20) и объявленные свойства автора вида `flag`; выпадающий список — `image` (картинки `files.images`),
 * `rotation` (0/90/180/270 в плоской сцене; в трёхмерной — любое число, текстовое поле, «Фаза 15», требование 29),
 * `shape` (`box`/`cylinder`/`capsule`/`sphere`), `follow_mouse` (`x`/`y`/`xy`); `color` — палитра; объявленное свойство
 * автора вида `text` — строковое поле без разбора JSON («Таблицы данных», требование 40); остальное —
 * текст. Значение, которого нет в наборе поля (`rotation: 45`, `collides: 1`, картинки нет в списке), —
 * текстовое поле. `walk_to` и `walk_speed` (Фаза 11, требование 45) — как другие пары и числа,
 * `on_click` — как `keys`, а `height` — как другие числа: всё это уже текст по умолчанию, отдельного вида не заводится.
 */
export function propertyFieldKind(
  key: string,
  value: unknown,
  authorPropertyKinds: Readonly<Record<string, string>>,
  imageNames: readonly string[],
  isThreeDimensionalScene: boolean,
): PropertyFieldKind {
  if (authorPropertyKinds[key] === "text") return { kind: "raw-text" };
  if (key === "collides" || key === "camera_follows" || key === "flip_x" || authorPropertyKinds[key] === "flag") {
    return typeof value === "boolean" ? { kind: "checkbox" } : { kind: "text" };
  }
  if (key === "image") {
    return typeof value === "string" && imageNames.includes(value) ? { kind: "select", options: imageNames } : { kind: "text" };
  }
  if (key === "shape") {
    return typeof value === "string" && (SHAPE_OPTIONS as readonly string[]).includes(value)
      ? { kind: "select", options: SHAPE_OPTIONS }
      : { kind: "text" };
  }
  if (key === "rotation" && !isThreeDimensionalScene) {
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
