import { particleCarrierAt } from "./particleCarrierAt";
import type { CanvasRect } from "./selectionDrawing";

/**
 * Объект без картинки и цвета под точкой холста: `object_at` движка такие не находит — они не рисуются, — а источник
 * дыма или искр бывает именно таким. Из нескольких выше тот, у кого `layer` больше, при равных — стоящий в списке позже.
 * Объект, которому частицы не дописать (`repeat_x`, нет `position` или `size`), пропускается.
 */
export function fillLessObjectAt(
  x: number,
  y: number,
  objectIds: readonly number[],
  objectProperties: (id: number) => Record<string, unknown> | null,
  objectRect: (id: number) => CanvasRect | undefined,
): number | undefined {
  return particleCarrierAt(x, y, objectIds, objectProperties, objectRect, (properties) => properties.image === undefined && properties.color === undefined);
}
