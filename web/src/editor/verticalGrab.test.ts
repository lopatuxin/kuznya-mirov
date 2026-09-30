import { describe, expect, it } from "vitest";
import type { SpaceProjection } from "./handleGeometry";
import { computeHeight } from "./handleMath";
import type { Vec2 } from "./objectPlacement";
import { createVerticalGrab, raisedAtPointer } from "./verticalGrab";

type Vec3 = [number, number, number];

const VIEWPORT: Vec2 = [1280, 800];
const FOCAL = VIEWPORT[1] / 2 / Math.tan((35 / 2) * (Math.PI / 180));

function dot(a: readonly number[], b: readonly number[]): number {
  return a.reduce((sum, value, index) => sum + value * (b[index] as number), 0);
}

/** Честная камера с перспективой, как у движка: наклон, поворот, `distance` клеток от точки земли `target` до глаза. */
function pinholeCamera(target: Vec2, yaw: number, pitch: number, distance: number) {
  const [sinPitch, cosPitch] = [Math.sin((pitch * Math.PI) / 180), Math.cos((pitch * Math.PI) / 180)];
  const [sinYaw, cosYaw] = [Math.sin((yaw * Math.PI) / 180), Math.cos((yaw * Math.PI) / 180)];
  const eye: Vec3 = [target[0] - distance * cosPitch * sinYaw, target[1] + distance * cosPitch * cosYaw, distance * sinPitch];
  const forward: Vec3 = [cosPitch * sinYaw, -cosPitch * cosYaw, -sinPitch];
  const up: Vec3 = [sinPitch * sinYaw, -sinPitch * cosYaw, cosPitch];
  const right: Vec3 = [cosYaw, sinYaw, 0];
  const relative = (point: Vec3): Vec3 => [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
  const projection: SpaceProjection = {
    screenPoint: (x, y, z) => {
      const rel = relative([x, y, z]);
      const depth = dot(rel, forward);
      if (depth <= 1e-6) return undefined;
      return [VIEWPORT[0] / 2 + (FOCAL * dot(rel, right)) / depth, VIEWPORT[1] / 2 - (FOCAL * dot(rel, up)) / depth];
    },
  };
  const rayDirection = (pointer: Vec2): Vec3 => {
    const dx = (pointer[0] - VIEWPORT[0] / 2) / FOCAL;
    const dy = (pointer[1] - VIEWPORT[1] / 2) / FOCAL;
    return [0, 1, 2].map((index) => forward[index]! + right[index]! * dx - up[index]! * dy) as Vec3;
  };
  return { eye, projection, rayDirection };
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
