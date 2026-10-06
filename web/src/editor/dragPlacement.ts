const DRAG_START_THRESHOLD_PX = 4;

/** Сдвиг указателя дальше 4 пикселей начинает перенос — требование 7. */
export function hasCrossedDragThreshold(deltaX: number, deltaY: number): boolean {
  return Math.hypot(deltaX, deltaY) > DRAG_START_THRESHOLD_PX;
}
