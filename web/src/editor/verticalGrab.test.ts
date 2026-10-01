import { describe, expect, it } from "vitest";
import { computeHeight } from "./handleMath";
import type { Vec2 } from "./objectPlacement";
import { pinholeCamera } from "./pinholeCamera";
import { createPlaneGrab, createVerticalGrab, pointOnPlane, raisedAtPointer } from "./verticalGrab";

function dot(a: readonly number[], b: readonly number[]): number {
  return a.reduce((sum, value, index) => sum + value * (b[index] as number), 0);
}

function pointAt(camera: ReturnType<typeof pinholeCamera>, x: number, y: number, z: number): Vec2 {
  return camera.projection.screenPoint(x, y, z) as Vec2;
}

/** Высота точки вертикали `(x, y)`, ближайшей в мире к лучу через `pointer`, — перебором, без формул. */
function bruteForceNearestHeight(camera: ReturnType<typeof pinholeCamera>, x: number, y: number, pointer: Vec2): number {
  const direction = camera.rayDirection(pointer);
  const lengthSquared = dot(direction, direction);
  const distanceSquared = (z: number): number => {
    const rel = [x - camera.eye[0], y - camera.eye[1], z - camera.eye[2]];
    const along = dot(rel, direction) / lengthSquared;
    return dot(rel, rel) - along * along * lengthSquared;
  };
  let low = -30;
  let high = 30;
  for (let step = 0; step < 200; step++) {
    const first = low + (high - low) / 3;
    const second = high - (high - low) / 3;
    if (distanceSquared(first) < distanceSquared(second)) high = second;
    else low = first;
  }
  return (low + high) / 2;
}

const GROUND: Vec2 = [11, 9.5];
const BASE = 1;
// Близкая камера под небольшим наклоном — сильная перспектива: вертикаль заметно сужается вверх.
const STRONG = pinholeCamera([10, 10], 30, 25, 8);

function oldLinearHeight(camera: ReturnType<typeof pinholeCamera>, pointerStart: Vec2, pointerNow: Vec2): number {
  const unit = [pointAt(camera, GROUND[0], GROUND[1], BASE + 1)[0] - pointerStart[0], pointAt(camera, GROUND[0], GROUND[1], BASE + 1)[1] - pointerStart[1]];
  return BASE + ((pointerNow[0] - pointerStart[0]) * unit[0]! + (pointerNow[1] - pointerStart[1]) * unit[1]!) / dot(unit, unit);
}

describe("указатель на проекции точки вертикали", () => {
  const start = pointAt(STRONG, GROUND[0], GROUND[1], BASE);
  const grab = createVerticalGrab(STRONG.projection, GROUND, BASE);

  it.each([-1.5, 0.5, 1, 2.25, 3.37, 5])("даёт ровно высоту основания + %s при сильной перспективе", (rise) => {
    const now = pointAt(STRONG, GROUND[0], GROUND[1], BASE + rise);
    expect(computeHeight(BASE, grab!, start, now, false)).toBe(Math.round((BASE + rise) * 100) / 100);
  });

  it("прежний линейный способ на этой камере ошибался бы больше десятой клетки", () => {
    const now = pointAt(STRONG, GROUND[0], GROUND[1], BASE + 5);
    expect(Math.abs(oldLinearHeight(STRONG, start, now) - (BASE + 5))).toBeGreaterThan(0.1);
  });

  it("с Ctrl — до целой клетки", () => {
    const now = pointAt(STRONG, GROUND[0], GROUND[1], BASE + 2.4);
    expect(computeHeight(BASE, grab!, start, now, true)).toBe(3);
  });

  it("камера строго сверху, но не над объектом: вертикаль — луч от центра, высота по-прежнему точная", () => {
    const camera = pinholeCamera([10, 10], 0, 90, 8);
    const topGrab = createVerticalGrab(camera.projection, GROUND, BASE);
    const now = pointAt(camera, GROUND[0], GROUND[1], BASE + 2.5);
    expect(computeHeight(BASE, topGrab!, pointAt(camera, GROUND[0], GROUND[1], BASE), now, false)).toBe(3.5);
  });
});

describe("указатель не на вертикали", () => {
  it("высота — у точки вертикали, ближайшей в мире к лучу указателя", () => {
    const grab = createVerticalGrab(STRONG.projection, GROUND, BASE)!;
    for (const [dx, dy, rise] of [[40, 25, 2], [-60, 10, 4], [15, -90, -1], [120, 80, 3]] as const) {
      const pointer: Vec2 = [pointAt(STRONG, GROUND[0], GROUND[1], BASE + rise)[0] + dx, pointAt(STRONG, GROUND[0], GROUND[1], BASE + rise)[1] + dy];
      expect(BASE + (raisedAtPointer(grab, pointer) as number)).toBeCloseTo(bruteForceNearestHeight(STRONG, GROUND[0], GROUND[1], pointer), 6);
    }
  });

  it("без восстановленной камеры берётся ближайшая на экране точка — на самой вертикали она та же", () => {
    const grab = createVerticalGrab(STRONG.projection, GROUND, BASE)!;
    const onLine = pointAt(STRONG, GROUND[0], GROUND[1], BASE + 3.5);
    expect(raisedAtPointer({ ...grab, ray: null }, onLine)).toBeCloseTo(raisedAtPointer(grab, onLine) as number, 9);
    expect(raisedAtPointer({ ...grab, ray: null }, onLine)).toBeCloseTo(3.5, 9);
  });
});

describe("вырожденные случаи", () => {
  const grab = createVerticalGrab(STRONG.projection, GROUND, BASE)!;
  const start = pointAt(STRONG, GROUND[0], GROUND[1], BASE);
  // Точка схода вертикалей — там, где глаз видит прямо вниз.
  const vanishing = pointAt(STRONG, STRONG.eye[0], STRONG.eye[1], STRONG.eye[2] - 1);

  it("указатель в точке схода вертикали высоту не меняет", () => {
    expect(computeHeight(BASE, grab, start, vanishing, false)).toBe(BASE);
  });

  it("указатель за точкой схода — точка вертикали была бы за камерой — высоту не меняет", () => {
    const outward: Vec2 = [vanishing[0] - start[0], vanishing[1] - start[1]];
    const length = Math.hypot(outward[0], outward[1]);
    const beyond: Vec2 = [vanishing[0] + (outward[0] / length) * 300, vanishing[1] + (outward[1] / length) * 300];
    const height = computeHeight(BASE, grab, start, beyond, false);
    expect(Number.isFinite(height)).toBe(true);
    expect(height).toBe(BASE);
  });

  it("камера строго сверху над объектом: вертикаль видна в точку, хвата нет", () => {
    const overhead = pinholeCamera(GROUND, 0, 90, 8);
    expect(createVerticalGrab(overhead.projection, GROUND, BASE)).toBeUndefined();
  });

  it("середина или точка вертикали за камерой — хвата нет", () => {
    expect(createVerticalGrab({ screenPoint: () => undefined }, GROUND, BASE)).toBeUndefined();
    expect(createVerticalGrab({ screenPoint: (_x, _y, z) => (z > BASE + 1 ? undefined : [z, z * 2]) }, GROUND, BASE)).toBeUndefined();
  });
});

describe("место горизонтальной плоскости под указателем", () => {
  const anchor: [number, number, number] = [11, 9.5, 0];

  it.each([
    [STRONG, "близкая камера"],
    [pinholeCamera([20, 15], 200, 50, 40), "дальняя камера, повёрнутая"],
    [pinholeCamera([20, 15], 0, 90, 30), "камера строго сверху"],
  ])("луч через экранную точку места на высоте h даёт это место: %#", (camera) => {
    const grab = createPlaneGrab(camera.projection, anchor);
    expect(grab).toBeDefined();
    for (const [x, y, height] of [[10, 8, 0], [14.5, 12, 3.5], [8, 14, -2]] as const) {
      const place = pointOnPlane(grab!, pointAt(camera, x, y, height), height);
      expect(place?.[0]).toBeCloseTo(x, 6);
      expect(place?.[1]).toBeCloseTo(y, 6);
    }
  });

  it("плоскость за камерой или луч параллелен ей — места нет", () => {
    const grab = createPlaneGrab(STRONG.projection, anchor)!;
    expect(pointOnPlane(grab, pointAt(STRONG, 10, 8, 0), STRONG.eye[2] + 5)).toBeUndefined();
    const horizon = pinholeCamera([20, 15], 0, 5, 30);
    const horizonGrab = createPlaneGrab(horizon.projection, anchor)!;
    expect(pointOnPlane(horizonGrab, [640, 200], 0)).toBeUndefined();
  });

  it("камеру восстановить нельзя — хвата нет", () => {
    expect(createPlaneGrab({ screenPoint: () => undefined }, anchor)).toBeUndefined();
    const overhead = pinholeCamera([anchor[0], anchor[1]], 0, 90, 30);
    expect(createPlaneGrab(overhead.projection, anchor)).toBeUndefined();
  });
});
