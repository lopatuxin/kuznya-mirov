import type { SpaceProjection } from "./handleGeometry";
import type { Vec2 } from "./objectPlacement";

type Vec3 = [number, number, number];

const VIEWPORT: Vec2 = [1280, 800];
const FOCAL = VIEWPORT[1] / 2 / Math.tan((35 / 2) * (Math.PI / 180));

function dot(a: readonly number[], b: readonly number[]): number {
  return a.reduce((sum, value, index) => sum + value * (b[index] as number), 0);
}

/**
 * Для тестов: честная камера с перспективой, как у движка, — наклон, поворот, `distance` клеток от точки
 * земли `target` до глаза. `projection` — тот же `screen_point`, `rayDirection` — луч через точку экрана.
 */
export function pinholeCamera(target: Vec2, yaw: number, pitch: number, distance: number) {
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
