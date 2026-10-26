import type { Vec2 } from "./objectPlacement";
import { toVec3, type Vec3 } from "./terrainReadings";

/** Ось дороги — ломаная от начала к концу; `z` каждой точки — высота профиля в ней. */
type RoadAxis = readonly Vec3[];

/** Связи дороги, как их отдаёт `road_links` («Редактор», «Вызовы движка»): точки сцены с высотой дороги в них. */
export type RoadLinks = {
  axis: RoadAxis;
  /** Изгиб оси в градусах после допустимых пределов; положительный поворачивает в сторону положительного `rotation`. */
  bend: number;
  /** Середина оси по длине — там ручка изгиба. */
  handle: Vec3;
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
const BEND_HANDLE_COLOR = "#ffc83d";
const BEND_HANDLE_HOVER_COLOR = "#ffffff";
// Тёмная кайма под светлой линией — видна и на светлой, и на тёмной земле.
const HALO_COLOR = "rgba(6, 8, 14, 0.75)";
/** Толщина одна на экране при любом приближении: линия рисуется в точках холста, а не клетках сцены. */
const LINE_WIDTH_PX = 1;
const HALO_EXTRA_PX = 2;
const LINK_RADIUS_PX = 4;
const END_HALF_SIDE_PX = 4;
/** Круглая ручка изгиба крупнее точки связи: её берут мышью. */
export const BEND_HANDLE_RADIUS_PX = 7;

function readAxis(value: unknown): RoadAxis | undefined {
  if (!Array.isArray(value) || value.length < 2) return undefined;
  const points = value.map(toVec3);
  return points.every((point) => point !== undefined) ? (points as Vec3[]) : undefined;
}

/** Читает каждый элемент списка и оставляет прочитанные; не список — пусто. */
function readEach<T>(value: unknown, read: (item: unknown) => T | undefined): T[] {
  if (!Array.isArray(value)) return [];
  return value.map(read).filter((item): item is T => item !== undefined);
}

/** Ответ `road_links` как типизированные связи; `null` — не дорога, игры нет или ответ непохож на связи. */
export function readRoadLinks(value: unknown): RoadLinks | null {
  const raw = value as { axis?: unknown; bend?: unknown; handle?: unknown; links?: unknown; ends?: unknown; neighbours?: unknown } | null | undefined;
  if (raw === null || raw === undefined) return null;
  const axis = readAxis(raw.axis);
  const handle = toVec3(raw.handle);
  if (axis === undefined || handle === undefined || typeof raw.bend !== "number" || !Number.isFinite(raw.bend)) return null;
  return {
    axis,
    bend: raw.bend,
    handle,
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

/** Ломаная по точкам оси; точка за камерой разрывает линию, и следующая видимая начинает её заново. */
function drawAxis(context: CanvasRenderingContext2D, axis: readonly (Vec2 | undefined)[], color: string): void {
  strokeWithHalo(context, color, () => {
    let isPenDown = false;
    for (const point of axis) {
      if (point === undefined) {
        isPenDown = false;
      } else if (isPenDown) {
        context.lineTo(point[0], point[1]);
      } else {
        context.moveTo(point[0], point[1]);
        isPenDown = true;
      }
    }
  });
}

/**
 * Связи выбранной дороги поверх трёхмерной сцены — «Редактор», Фаза 38, требование 32, Фаза 53, требование 29: её
 * ось дугой, оси связанных дорог, кружки в точках связей и квадратик у конца оси без связи — каждая точка на высоте
 * профиля. `project` ставит точку сцены на холст камерой, что видна сейчас; точка за камерой (`undefined`) её не
 * рисует.
 */
export function drawRoadLinks(context: CanvasRenderingContext2D, links: RoadLinks, project: (point: Vec3) => Vec2 | undefined, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineCap = "round";
  context.lineJoin = "round";
  for (const neighbour of links.neighbours) drawAxis(context, neighbour.map(project), NEIGHBOUR_COLOR);
  drawAxis(context, links.axis.map(project), AXIS_COLOR);
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

/** Круглая ручка изгиба в точке холста `point` — поверх ручек переноса: у прямой дороги она стоит там же, где их центр. */
export function drawBendHandle(context: CanvasRenderingContext2D, point: Vec2, isHighlighted: boolean, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.beginPath();
  context.arc(point[0], point[1], BEND_HANDLE_RADIUS_PX, 0, Math.PI * 2);
  context.strokeStyle = HALO_COLOR;
  context.lineWidth = LINE_WIDTH_PX + HALO_EXTRA_PX;
  context.stroke();
  context.fillStyle = isHighlighted ? BEND_HANDLE_HOVER_COLOR : BEND_HANDLE_COLOR;
  context.fill();
  context.restore();
}
