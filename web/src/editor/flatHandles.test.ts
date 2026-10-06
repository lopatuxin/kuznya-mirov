import { describe, expect, it } from "vitest";
import { computeFlatTranslateGeometry, flatScaleHandlePoints, hitTestFlatScaleHandles } from "./flatHandles";
import { hitTestHandles } from "./handleGeometry";

const RECT = { x: 100, y: 200, width: 200, height: 100 };

describe("computeFlatTranslateGeometry", () => {
  it("середина прямоугольника, стрелки вправо и вниз по 90 точек", () => {
    expect(computeFlatTranslateGeometry(RECT)).toEqual({ center: [200, 250], tipX: [290, 250], tipY: [200, 340], tipZ: null, ring: [] });
  });
});

describe("попадание в ручки переноса", () => {
  const geometry = computeFlatTranslateGeometry(RECT);

  it("квадрат в середине важнее стрелок", () => {
    expect(hitTestHandles(geometry, "translate", [208, 250])).toBe("center");
  });

  it("стрелки: вправо — x, вниз — y; мимо — ничего", () => {
    expect(hitTestHandles(geometry, "translate", [260, 252])).toBe("axis-x");
    expect(hitTestHandles(geometry, "translate", [198, 300])).toBe("axis-y");
    expect(hitTestHandles(geometry, "translate", [240, 300])).toBeNull();
  });
});

describe("flatScaleHandlePoints", () => {
  it("четыре угла и середины четырёх сторон рамки", () => {
    expect(flatScaleHandlePoints(RECT)).toEqual({
      left: [100, 250],
      right: [300, 250],
      top: [200, 200],
      bottom: [200, 300],
      "top-left": [100, 200],
      "top-right": [300, 200],
      "bottom-left": [100, 300],
      "bottom-right": [300, 300],
    });
  });
});

describe("попадание в ручки масштаба", () => {
  it("в 8 точках от ручки — попадание, дальше — нет", () => {
    expect(hitTestFlatScaleHandles(RECT, [300, 258])).toBe("right");
    expect(hitTestFlatScaleHandles(RECT, [300, 259])).toBeNull();
    expect(hitTestFlatScaleHandles(RECT, [205, 195])).toBe("top");
  });

  it("угол важнее стороны", () => {
    const small = { x: 0, y: 0, width: 10, height: 10 };

    expect(hitTestFlatScaleHandles(small, [0, 2])).toBe("top-left");
    expect(hitTestFlatScaleHandles(small, [0, 5])).toBe("top-left");
  });

  it("у крошечного объекта ручки накрывают его целиком: нажатие в середине попадает в ближнюю ручку", () => {
    const tiny = { x: 50, y: 50, width: 2, height: 2 };

    expect(hitTestFlatScaleHandles(tiny, [51, 51])).not.toBeNull();
  });

  it("далеко от рамки — ничего", () => {
    expect(hitTestFlatScaleHandles(RECT, [200, 250])).toBeNull();
  });
});
