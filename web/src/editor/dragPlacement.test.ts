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
