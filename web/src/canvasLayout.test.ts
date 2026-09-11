import { describe, expect, it } from "vitest";
import { computeCanvasLayout } from "./canvasLayout";

describe("computeCanvasLayout", () => {
  it("держит css-размер равным окну целиком, а не вписанному прямоугольнику сцены", () => {
    const layout = computeCanvasLayout(1470, 690, 1);
    expect(layout.cssWidth).toBe(1470);
    expect(layout.cssHeight).toBe(690);
  });

  it("умножает буфер на devicePixelRatio, а css-размер оставляет в css-пикселях", () => {
    const layout = computeCanvasLayout(1470, 690, 2);
    expect(layout.cssWidth).toBe(1470);
    expect(layout.cssHeight).toBe(690);
    expect(layout.bufferWidth).toBe(2940);
    expect(layout.bufferHeight).toBe(1380);
  });

  it("округляет дробный буфер, оставшийся от нецелого devicePixelRatio", () => {
    const layout = computeCanvasLayout(375, 812, 2.5);
    expect(layout.bufferWidth).toBe(Math.round(375 * 2.5));
    expect(layout.bufferHeight).toBe(Math.round(812 * 2.5));
  });

  it("не отдаёт нулевой или отрицательный размер буфера на вырожденном окне", () => {
    const layout = computeCanvasLayout(0, 0, 1);
    expect(layout.bufferWidth).toBeGreaterThanOrEqual(1);
    expect(layout.bufferHeight).toBeGreaterThanOrEqual(1);
  });
});
