import { describe, expect, it } from "vitest";
import {
  ARROW_LENGTH_PX,
  arrowVectorOfHit,
  cellsPerScreenPixel,
  computeHandleGeometry,
  hitTestHandles,
  scaleAxisOfHit,
  type SpaceProjection,
} from "./handleGeometry";
import type { ObjectPlacement } from "./objectPlacement";

/** Камера сверху без перспективы: `pixelsPerCell` точек на клетку, высота уходит вверх по экрану. */
function topDownProjection(pixelsPerCell: number, yaw = 0): SpaceProjection {
  const cos = Math.cos((yaw * Math.PI) / 180);
  const sin = Math.sin((yaw * Math.PI) / 180);
  return {
    screenPoint: (x, y, z) => [(x * cos - y * sin) * pixelsPerCell + 50, (x * sin + y * cos) * pixelsPerCell + 50 - z * pixelsPerCell],
  };
}

const BOX: ObjectPlacement = { position: [3, 4], size: [2, 2], height: 2, rotation: null, hasShape: true };

describe("cellsPerScreenPixel", () => {
  it("столько клеток в точке, сколько их на самом деле, при любом повороте камеры", () => {
    expect(cellsPerScreenPixel(topDownProjection(10), [4, 5], [90, 100])).toBeCloseTo(0.1);
    expect(cellsPerScreenPixel(topDownProjection(40, 37), [4, 5], [90, 100])).toBeCloseTo(0.025);
  });

  it("шаг за камерой — ничего", () => {
    expect(cellsPerScreenPixel({ screenPoint: () => undefined }, [4, 5], [90, 100])).toBeUndefined();
  });
});

describe("computeHandleGeometry", () => {
  it("стрелки одной длины на экране при любом приближении — «Редактор», требование 9", () => {
    for (const pixelsPerCell of [4, 10, 60]) {
      const geometry = computeHandleGeometry(topDownProjection(pixelsPerCell), BOX, "translate");
      expect(geometry).toBeDefined();
      const arrow = arrowVectorOfHit(geometry!, "axis-x")!;
      expect(Math.hypot(arrow[0], arrow[1])).toBeCloseTo(ARROW_LENGTH_PX, 1);
    }
  });

  it("перенос — стрелки по осям сцены, без высоты; середина объекта на земле — середина ручек", () => {
    const geometry = computeHandleGeometry(topDownProjection(10), BOX, "translate")!;
    expect(geometry.center).toEqual([90, 100]);
    expect(arrowVectorOfHit(geometry, "axis-x")![1]).toBeCloseTo(0);
    expect(arrowVectorOfHit(geometry, "axis-y")![0]).toBeCloseTo(0);
    expect(geometry.tipZ).toBe(null);
  });

  it("масштаб — стрелки вдоль сторон повёрнутого объекта и высота вверх у фигуры", () => {
    const turned = computeHandleGeometry(topDownProjection(10), { ...BOX, rotation: 90 }, "scale")!;
    const width = arrowVectorOfHit(turned, "axis-x")!;
    expect(width[0]).toBeCloseTo(0);
    expect(width[1]).toBeCloseTo(ARROW_LENGTH_PX);
    const height = arrowVectorOfHit(turned, "axis-z")!;
    expect(height[1]).toBeLessThan(0);
  });

  it("у плоского объекта на земле ручки высоты нет", () => {
    const geometry = computeHandleGeometry(topDownProjection(10), { ...BOX, hasShape: false, height: null }, "scale")!;
    expect(geometry.tipZ).toBe(null);
    expect(hitTestHandles(geometry, "scale", [90, 40])).toBe(null);
  });

  it("поворот — кольцо радиусом около 80 точек вокруг середины", () => {
    const geometry = computeHandleGeometry(topDownProjection(10), BOX, "rotate")!;
    for (const point of geometry.ring) {
      expect(Math.hypot(point[0] - 90, point[1] - 100)).toBeCloseTo(80, 1);
    }
  });

  it("середина за камерой — ручек нет", () => {
    expect(computeHandleGeometry({ screenPoint: () => undefined }, BOX, "translate")).toBeUndefined();
  });
});

describe("hitTestHandles", () => {
  const projection = topDownProjection(10);
  const translate = computeHandleGeometry(projection, BOX, "translate")!;

  it("попадание в каждую ручку переноса: стрелки осей и средний квадрат", () => {
    expect(hitTestHandles(translate, "translate", [150, 100])).toBe("axis-x");
    expect(hitTestHandles(translate, "translate", [90, 160])).toBe("axis-y");
    expect(hitTestHandles(translate, "translate", [92, 103])).toBe("center");
  });

  it("мимо — ничего; до конца стрелки допуск в несколько точек", () => {
    expect(hitTestHandles(translate, "translate", [150, 130])).toBe(null);
    expect(hitTestHandles(translate, "translate", [150, 106])).toBe("axis-x");
    expect(hitTestHandles(translate, "translate", [150, 112])).toBe(null);
  });

  it("середина важнее стрелки, у которой начало рядом", () => {
    expect(hitTestHandles(translate, "translate", [96, 100])).toBe("center");
  });

  it("кольцо: попадание на линии и мимо в середине", () => {
    const ring = computeHandleGeometry(projection, BOX, "rotate")!;
    expect(hitTestHandles(ring, "rotate", [170, 100])).toBe("ring");
    expect(hitTestHandles(ring, "rotate", [90, 100])).toBe(null);
    expect(hitTestHandles(ring, "rotate", [260, 100])).toBe(null);
  });

  it("масштаб: три оси и общая ручка", () => {
    const scale = computeHandleGeometry(projection, BOX, "scale")!;
    expect(hitTestHandles(scale, "scale", [150, 100])).toBe("axis-x");
    expect(hitTestHandles(scale, "scale", [90, 160])).toBe("axis-y");
    expect(hitTestHandles(scale, "scale", [90, 40])).toBe("axis-z");
    expect(hitTestHandles(scale, "scale", [90, 100])).toBe("center");
  });
});

describe("scaleAxisOfHit", () => {
  it("ось масштаба для схваченной ручки", () => {
    expect(scaleAxisOfHit("axis-x")).toBe("width");
    expect(scaleAxisOfHit("axis-y")).toBe("depth");
    expect(scaleAxisOfHit("axis-z")).toBe("height");
    expect(scaleAxisOfHit("center")).toBe("uniform");
    expect(scaleAxisOfHit("ring")).toBe(null);
  });
});
