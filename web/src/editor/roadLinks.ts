import type { Vec2 } from "./objectPlacement";
import { toVec3, type Vec3 } from "./terrainReadings";

type RoadAxis = readonly [Vec3, Vec3];

/** Связи дороги, как их отдаёт `road_links` («Редактор», «Вызовы движка»): точки сцены с высотой дороги. */
export type RoadLinks = {
  axis: RoadAxis;
  /** Точки, где дорога связана с соседней. */
  links: readonly Vec3[];
  /** Концы оси без связи — тупики. */
  ends: readonly Vec3[];
  /** Оси связанных дорог. */
  neighbours: readonly RoadAxis[];
};

const AXIS_COLOR = "#ffc83d";
const NEIGHBOUR_COLOR = "#f4f7fb";
const LINK_COLOR = "#ffc83d";
const END_COLOR = "#ff6b5e";
// Тёмная кайма под светлой линией — видна и на светлой, и на тёмной земле.
const HALO_COLOR = "rgba(6, 8, 14, 0.75)";
/** Толщина одна на экране при любом приближении: линия рисуется в точках холста, а не клетках сцены. */
const LINE_WIDTH_PX = 1;
const HALO_EXTRA_PX = 2;
const LINK_RADIUS_PX = 4;
const END_HALF_SIDE_PX = 4;

function readAxis(value: unknown): RoadAxis | undefined {
  if (!Array.isArray(value)) return undefined;
  const from = toVec3(value[0]);
  const to = toVec3(value[1]);
  return from === undefined || to === undefined ? undefined : [from, to];
}

/** Читает каждый элемент списка и оставляет прочитанные; не список — пусто. */
function readEach<T>(value: unknown, read: (item: unknown) => T | undefined): T[] {
  if (!Array.isArray(value)) return [];
  return value.map(read).filter((item): item is T => item !== undefined);
}

/** Ответ `road_links` как типизированные связи; `null` — не дорога, игры нет или ответ непохож на связи. */
export function readRoadLinks(value: unknown): RoadLinks | null {
  const raw = value as { axis?: unknown; links?: unknown; ends?: unknown; neighbours?: unknown } | null | undefined;
  if (raw === null || raw === undefined) return null;
  const axis = readAxis(raw.axis);
  if (axis === undefined) return null;
  return {
    axis,
    links: readEach(raw.links, (item) => toVec3((item as { point?: unknown } | null)?.point)),
    ends: readEach(raw.ends, toVec3),
    neighbours: readEach(raw.neighbours, (item) => readAxis((item as { axis?: unknown } | null)?.axis)),
  };
}

function strokeWithHalo(context: CanvasRenderingContext2D, color: string, trace: () => void): void {
  for (const [strokeColor, width] of [[HALO_COLOR, LINE_WIDTH_PX + HALO_EXTRA_PX], [color, LINE_WIDTH_PX]] as const) {
    context.beginPath();
    trace();
    context.strokeStyle = strokeColor;
    context.lineWidth = width;
    context.stroke();
  }
}

function drawAxis(context: CanvasRenderingContext2D, axis: readonly [Vec2 | undefined, Vec2 | undefined], color: string): void {
  const [from, to] = axis;
  if (from === undefined || to === undefined) return;
  strokeWithHalo(context, color, () => {
    context.moveTo(from[0], from[1]);
    context.lineTo(to[0], to[1]);
  });
}

/**
 * Связи выбранной дороги поверх трёхмерной сцены — «Редактор», Фаза 38, требование 32: её ось, оси связанных
 * дорог, кружки в точках связей и квадратик у конца оси без связи. `project` ставит точку сцены на холст
 * камерой, что видна сейчас; точка за камерой (`undefined`) её не рисует.
 */
export function drawRoadLinks(context: CanvasRenderingContext2D, links: RoadLinks, project: (point: Vec3) => Vec2 | undefined, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineCap = "round";
  for (const neighbour of links.neighbours) drawAxis(context, [project(neighbour[0]), project(neighbour[1])], NEIGHBOUR_COLOR);
  drawAxis(context, [project(links.axis[0]), project(links.axis[1])], AXIS_COLOR);
  for (const end of links.ends) {
    const point = project(end);
    if (point === undefined) continue;
    strokeWithHalo(context, END_COLOR, () => context.rect(point[0] - END_HALF_SIDE_PX, point[1] - END_HALF_SIDE_PX, END_HALF_SIDE_PX * 2, END_HALF_SIDE_PX * 2));
  }
  for (const link of links.links) {
    const point = project(link);
    if (point === undefined) continue;
    context.beginPath();
    context.arc(point[0], point[1], LINK_RADIUS_PX, 0, Math.PI * 2);
    context.strokeStyle = HALO_COLOR;
    context.lineWidth = LINE_WIDTH_PX + HALO_EXTRA_PX;
    context.stroke();
    context.fillStyle = LINK_COLOR;
    context.fill();
  }
  context.restore();
}
