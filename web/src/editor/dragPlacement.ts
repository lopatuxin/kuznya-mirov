/**
 * Место объекта при переносе мышью — «Редактор», требования 8–9. Сдвиг указателя в CSS-пикселях
 * делится на размер клетки (ширина `object_rect` объекта в начале переноса, делённая на его
 * `size[0]`) и складывается со сдвигом в клетках; свободно — округление до сотой клетки, с
 * зажатым Ctrl — до целой. `Math.round` на уже поделённых на 100 или 1 значениях не тянет за собой
 * хвост машинного округления — «Редактор», крайний случай про `3.57`, а не `3.5700000000000003`.
 */
export function computeDragPosition(
  startPosition: readonly [number, number],
  pointerDeltaPx: readonly [number, number],
  cellSizePx: number,
  snapToWholeCells: boolean,
): [number, number] {
  const round = (value: number): number => (snapToWholeCells ? Math.round(value) : Math.round(value * 100) / 100);
  const x = round(startPosition[0] + pointerDeltaPx[0] / cellSizePx);
  const y = round(startPosition[1] + pointerDeltaPx[1] / cellSizePx);
  return [x, y];
}

/** Размер клетки в CSS-пикселях — ширина прямоугольника объекта на начало переноса, делённая на `size[0]`. */
export function cellSizeFromObjectRect(objectRectWidthPx: number, sizeX: number): number {
  return objectRectWidthPx / sizeX;
}

const DRAG_START_THRESHOLD_PX = 4;

/** Сдвиг указателя дальше 4 пикселей начинает перенос — требование 7. */
export function hasCrossedDragThreshold(deltaX: number, deltaY: number): boolean {
  return Math.hypot(deltaX, deltaY) > DRAG_START_THRESHOLD_PX;
}
