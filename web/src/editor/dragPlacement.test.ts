import { describe, expect, it } from "vitest";
import { cellSizeFromObjectRect, computeDragPosition, hasCrossedDragThreshold } from "./dragPlacement";

describe("computeDragPosition", () => {
  // «Редактор», критерии готовности «Страница»: object_rect шириной 30, size [1, 1], position [2, 6].
  it("свободный перенос округляет до сотой клетки", () => {
    const cellSize = cellSizeFromObjectRect(30, 1);
    expect(computeDragPosition([2, 6], [47, -20], cellSize, false)).toEqual([3.57, 5.33]);
  });

  it("с Ctrl округляет до целой клетки", () => {
    const cellSize = cellSizeFromObjectRect(30, 1);
    expect(computeDragPosition([2, 6], [47, -20], cellSize, true)).toEqual([4, 5]);
  });

  it("дробное начальное место без Ctrl остаётся дробным до сотой", () => {
    const cellSize = cellSizeFromObjectRect(30, 1);
    expect(computeDragPosition([12.5, 3], [10, 0], cellSize, false)).toEqual([12.83, 3]);
  });

  it("дробное начальное место с Ctrl округляется до целого", () => {
    const cellSize = cellSizeFromObjectRect(30, 1);
    expect(computeDragPosition([12.5, 3], [10, 0], cellSize, true)).toEqual([13, 3]);
  });

  it("округлённое значение не тянет хвост машинного округления", () => {
    const cellSize = cellSizeFromObjectRect(30, 1);
    const [x] = computeDragPosition([2, 6], [47, -20], cellSize, false);
    expect(String(x)).toBe("3.57");
  });
});

describe("computeDragPosition для объекта слоя глубины", () => {
  // Фаза 28, требование 21: на паузе по камере сцены 160 × 24 холм `position [90, 11.5]`, `size [24, 8]`,
  // `parallax 0.25` нарисован от (105, 16); окно 1280 × 720, клетка 60 точек. Рамка стоит на нарисованном
  // месте, но перенос считает от записанного.
  const HILL_RECORDED_POSITION: [number, number] = [90, 11.5];
  const HILL_FRAME_WIDTH_PX = 24 * 60;

  it("указатель вправо на 2 клетки — записанное место сдвигается на 2 клетки, а не от рамки", () => {
    const cellSize = cellSizeFromObjectRect(HILL_FRAME_WIDTH_PX, 24);
    expect(cellSize).toBe(60);
    expect(computeDragPosition(HILL_RECORDED_POSITION, [120, 0], cellSize, false)).toEqual([92, 11.5]);
  });

  it("сдвиг по обеим осям и по ширине рамки, без места рамки на экране", () => {
    const cellSize = cellSizeFromObjectRect(HILL_FRAME_WIDTH_PX, 24);
    expect(computeDragPosition(HILL_RECORDED_POSITION, [-60, 30], cellSize, false)).toEqual([89, 12]);
    expect(computeDragPosition(HILL_RECORDED_POSITION, [120, 60], cellSize, true)).toEqual([92, 13]);
  });
});

describe("hasCrossedDragThreshold", () => {
  it("сдвиг в пределах 4 пикселей не начинает перенос", () => {
    expect(hasCrossedDragThreshold(2, 2)).toBe(false);
    expect(hasCrossedDragThreshold(4, 0)).toBe(false);
  });

  it("сдвиг дальше 4 пикселей начинает перенос", () => {
    expect(hasCrossedDragThreshold(5, 0)).toBe(true);
    expect(hasCrossedDragThreshold(3, 3)).toBe(true);
  });
});
