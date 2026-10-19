import { trimFloatNoise } from "./handleMath";
import type { Vec2 } from "./objectPlacement";
import { roundToHundredths } from "./terrainFile";

/** Модель игры из `model_sizes()`: имя из `files.models` и размер в клетках без `height` у объекта — `[x, y, height]`. */
export type ProjectModel = { name: string; size: readonly [number, number, number] };

function isModelSize(value: unknown): value is [number, number, number] {
  return Array.isArray(value) && value.length === 3 && value.every((side) => typeof side === "number" && Number.isFinite(side));
}

/** Ответ `model_sizes()` по порядку объявления; запись не того вида пропускается, до загрузки игры ответ пустой. */
export function readModelSizes(raw: unknown): ProjectModel[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry: unknown): ProjectModel[] => {
    const { name, size } = (entry ?? {}) as { name?: unknown; size?: unknown };
    return typeof name === "string" && isModelSize(size) ? [{ name, size }] : [];
  });
}

/**
 * Новый объект из модели — «Редактор», требование 23: `size` — место модели на земле без `height`, как в файле, а середина
 * этого места — в `middle`, записанном месте под указателем; `position` — левый верхний угол, на поверхности.
 */
export function createModelObject(model: ProjectModel, middle: Vec2): Record<string, unknown> {
  const size: [number, number] = [trimFloatNoise(model.size[0]), trimFloatNoise(model.size[1])];
  return {
    position: [roundToHundredths(middle[0] - size[0] / 2), roundToHundredths(middle[1] - size[1] / 2)],
    size,
    model: model.name,
  };
}
