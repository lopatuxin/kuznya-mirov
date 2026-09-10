import { describe, expect, it } from "vitest";
import { computeCanvasLayout } from "./canvasLayout";

describe("computeCanvasLayout", () => {
  it("на широком окне ограничивает размер по высоте и держит клетку квадратной", () => {
    const layout = computeCanvasLayout(24, 24, 1470, 690, 1);
    expect(layout.cssWidth).toBe(690);
    expect(layout.cssHeight).toBe(690);
  });

  it("на узком высоком окне ограничивает размер по ширине и держит клетку квадратной", () => {
    const layout = computeCanvasLayout(24, 24, 400, 800, 1);
    expect(layout.cssWidth).toBe(400);
    expect(layout.cssHeight).toBe(400);
  });

  it("сохраняет соотношение сторон сцены, а не окна, для неквадратной сцены", () => {
    const layout = computeCanvasLayout(32, 16, 600, 600, 1);
    expect(layout.cssWidth).toBe(600);
    expect(layout.cssHeight).toBe(300);
  });

  it("умножает буфер на devicePixelRatio, а css-размер оставляет в css-пикселях", () => {
    const layout = computeCanvasLayout(24, 24, 1470, 690, 2);
    expect(layout.cssWidth).toBe(690);
    expect(layout.bufferWidth).toBe(1380);
    expect(layout.bufferHeight).toBe(1380);
  });

  it("не отдаёт нулевой или отрицательный размер буфера на вырожденном окне", () => {
    const layout = computeCanvasLayout(24, 24, 0, 0, 1);
    expect(layout.bufferWidth).toBeGreaterThanOrEqual(1);
    expect(layout.bufferHeight).toBeGreaterThanOrEqual(1);
  });

  it("держит соотношение сторон в пределах погрешности округления буфера для разных форм окна", () => {
    const sceneAspect = 24 / 24;
    const cases: Array<[number, number]> = [
      [1470, 690],
      [690, 1470],
      [1920, 1080],
      [375, 812],
    ];
    for (const [viewportWidth, viewportHeight] of cases) {
      const layout = computeCanvasLayout(24, 24, viewportWidth, viewportHeight, 1);
      expect(layout.bufferWidth / layout.bufferHeight).toBeCloseTo(sceneAspect, 1);
      expect(layout.cssWidth).toBeLessThanOrEqual(viewportWidth);
      expect(layout.cssHeight).toBeLessThanOrEqual(viewportHeight);
    }
  });
});
