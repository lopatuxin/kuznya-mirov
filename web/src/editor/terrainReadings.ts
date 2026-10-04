import type { Vec2 } from "./objectPlacement";
import type { BrushGrid } from "./terrainBrush";

/** Место с высотой: `ground_at` и `terrain_at` отдают `[x, y, z]` — клетки сцены и высота в клетках. */
export type Vec3 = readonly [number, number, number];

export function toVec2(value: unknown): Vec2 | undefined {
  if (!Array.isArray(value) || typeof value[0] !== "number" || typeof value[1] !== "number") return undefined;
  return [value[0], value[1]];
}

/** Точка с высотой; движок, что отдал только два числа, — высота 0. */
export function toVec3(value: unknown): Vec3 | undefined {
  const flat = toVec2(value);
  if (flat === undefined) return undefined;
  const z = (value as unknown[])[2];
  return [flat[0], flat[1], typeof z === "number" ? z : 0];
}

/** Рельеф, каким его отдаёт `terrain_heights`: высоты файла без отпечатков, итоговые высоты с отпечатками и вода как есть. */
export type TerrainSnapshot = { grid: BrushGrid; effective: Float64Array; water: unknown };

export function readTerrainSnapshot(value: unknown): TerrainSnapshot | undefined {
  const snapshot = value as { density?: unknown; columns?: unknown; rows?: unknown; heights?: unknown; effective?: unknown; water?: unknown } | undefined;
  if (typeof snapshot?.density !== "number" || typeof snapshot.columns !== "number" || typeof snapshot.rows !== "number") return undefined;
  if (!(snapshot.heights instanceof Float64Array) || !(snapshot.effective instanceof Float64Array)) return undefined;
  return {
    grid: { density: snapshot.density, columns: snapshot.columns, rows: snapshot.rows, heights: snapshot.heights },
    effective: snapshot.effective,
    water: snapshot.water,
  };
}
