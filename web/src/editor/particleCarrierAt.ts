import type { CanvasRect } from "./selectionDrawing";

/** «Проверка перед запуском», требование 27: свойства частиц — только у объекта с `position` и `size` и без `repeat_x`; `repeat_x: false` движок считает отсутствующим. */
export function canCarryParticles(properties: Record<string, unknown>): boolean {
  return properties.position !== undefined && properties.size !== undefined && (properties.repeat_x === undefined || properties.repeat_x === false);
}

/**
 * Самый верхний из объектов под точкой холста, которым можно дописать свойства частиц; остальные пропускаются, будто их нет.
 * Из нескольких выше тот, у кого `layer` больше, при равных — стоящий в списке позже. `isAccepted` сужает выбор дальше.
 */
export function particleCarrierAt(
  x: number,
  y: number,
  objectIds: readonly number[],
  objectProperties: (id: number) => Record<string, unknown> | null,
  objectRect: (id: number) => CanvasRect | undefined,
  isAccepted: (properties: Record<string, unknown>) => boolean = () => true,
): number | undefined {
  let best: { id: number; layer: number } | undefined;
  for (const id of objectIds) {
    const properties = objectProperties(id);
    if (properties === null || !canCarryParticles(properties) || !isAccepted(properties)) continue;
    const rect = objectRect(id);
    if (rect === undefined || x < rect.x || x >= rect.x + rect.width || y < rect.y || y >= rect.y + rect.height) continue;
    const layer = typeof properties.layer === "number" ? properties.layer : 0;
    if (best === undefined || layer >= best.layer) best = { id, layer };
  }
  return best?.id;
}
