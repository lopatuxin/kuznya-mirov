import type { Engine } from "engine";
import type { ImprintContext } from "./imprintSceneController";
import { readObjectPlacement, type PlacementChange, type Vec2 } from "./objectPlacement";
import type { PaintContext } from "./paintSceneController";
import { bendThroughPoint } from "./roadBendMath";
import { BEND_HANDLE_RADIUS_PX, drawBendHandle, readRoadLinks, type RoadLinks } from "./roadLinks";
import type { BrushSettings } from "./terrainBrush";
import { toVec2, toVec3 } from "./terrainReadings";
import { createPlaneGrab, pointOnPlane, type PlaneGrab } from "./verticalGrab";

/** Вызовы движка, которыми пользуется ручка изгиба: связи дороги, запись `bend` в собранный мир и камера. */
export type RoadBendSceneEngine = Pick<Engine, "road_links" | "set_property" | "remove_property" | "screen_point" | "ground_at">;

/** Что ручка изгиба узнаёт у основного контроллера при каждом событии; контекст трёхмерной сцены подходит как есть. */
export type RoadBendSceneContext = {
  engine: RoadBendSceneEngine;
  areHandlesAvailable: boolean;
  selectedIndex: number | null;
  brush: BrushSettings | null;
  paint: PaintContext | null;
  imprints: Pick<ImprintContext, "placing">;
  getObjectProperties: (objectId: number) => Record<string, unknown> | null;
  /** Отпускание после жеста: `bend` — одно действие, как у других ручек. */
  onCommitPlacement: (objectIndex: number, changes: PlacementChange[]) => void;
};

type RoadBendPointer = { pointerId: number; buttons: number; x: number; y: number };

type RoadBendGesture = {
  pointerId: number;
  objectIndex: number;
  /** Концы хорды: они стоят при любом изгибе. */
  chord: readonly [Vec2, Vec2];
  width: number;
  /** Горизонтальная плоскость на высоте ручки: по ней указатель ищет место; `null` — камеру не восстановить, берётся земля. */
  plane: PlaneGrab | null;
  planeHeight: number;
  /** `bend` в файле до жеста; `undefined` — свойства не было, отмена его убирает. */
  previous: number | undefined;
  /** Последний `bend`, что принял движок. */
  last: number;
  hasSent: boolean;
};

export type RoadBendSceneController = {
  /** Нажатие на круглую ручку выбранной дороги; `true` — жест начат. */
  start: (context: RoadBendSceneContext, input: RoadBendPointer) => boolean;
  /** Движение указателя; `true` — идёт жест, остальное ему не нужно. */
  pointerMove: (context: RoadBendSceneContext, input: RoadBendPointer) => boolean;
  pointerUp: (context: RoadBendSceneContext, input: RoadBendPointer) => boolean;
  pointerCancel: (context: RoadBendSceneContext, input: RoadBendPointer) => boolean;
  /** Esc: жест, если он идёт, возвращает прежний изгиб; `true` — жест был. */
  cancel: (context: RoadBendSceneContext) => boolean;
  /** Жест бросается без возврата изгиба: мир уже собран заново. */
  abandon: () => void;
  isActive: () => boolean;
  /** Ручка поверх ручек переноса; `pointer` — где указатель над холстом, под ним ручка светлеет. */
  drawHandle: (context: RoadBendSceneContext, canvas: CanvasRenderingContext2D, links: RoadLinks, pointer: Vec2 | null, pixelRatio: number) => void;
};

/** Ручку берут чуть шире, чем она нарисована. */
const HIT_MARGIN_PX = 2;
const LEFT_BUTTON_BIT = 1;
const BEND_KEY = "bend";

function canBend(context: RoadBendSceneContext): context is RoadBendSceneContext & { selectedIndex: number } {
  return context.areHandlesAvailable && context.selectedIndex !== null && context.brush === null && context.paint === null && context.imprints.placing === null;
}

function distance(first: Vec2, second: Vec2): number {
  return Math.hypot(first[0] - second[0], first[1] - second[1]);
}

function handleOnScreen(engine: RoadBendSceneEngine, links: RoadLinks): Vec2 | undefined {
  return toVec2(engine.screen_point(links.handle[0], links.handle[1], links.handle[2]));
}

export function createRoadBendSceneController(): RoadBendSceneController {
  let gesture: RoadBendGesture | null = null;

  function placeUnderPointer(engine: RoadBendSceneEngine, active: RoadBendGesture, input: RoadBendPointer): Vec2 | undefined {
    const point: Vec2 = [input.x, input.y];
    if (active.plane !== null) return pointOnPlane(active.plane, point, active.planeHeight);
    const ground = toVec3(engine.ground_at(point[0], point[1]));
    return ground === undefined ? undefined : [ground[0], ground[1]];
  }

  function restore(engine: RoadBendSceneEngine, active: RoadBendGesture): void {
    if (!active.hasSent) return;
    if (active.previous === undefined) engine.remove_property(active.objectIndex, BEND_KEY);
    else engine.set_property(active.objectIndex, BEND_KEY, active.previous);
  }

  function start(context: RoadBendSceneContext, input: RoadBendPointer): boolean {
    if (!canBend(context)) return false;
    const links = readRoadLinks(context.engine.road_links(context.selectedIndex));
    const handle = links === null ? undefined : handleOnScreen(context.engine, links);
    if (links === null || handle === undefined || distance(handle, [input.x, input.y]) > BEND_HANDLE_RADIUS_PX + HIT_MARGIN_PX) return false;
    const properties = context.getObjectProperties(context.selectedIndex);
    const placement = readObjectPlacement(properties);
    if (placement === null) return false;
    const previous = typeof properties?.[BEND_KEY] === "number" ? (properties[BEND_KEY] as number) : undefined;
    const projection = { screenPoint: (x: number, y: number, z: number) => toVec2(context.engine.screen_point(x, y, z)) };
    const [first, last] = [links.axis[0], links.axis[links.axis.length - 1]];
    gesture = {
      pointerId: input.pointerId,
      objectIndex: context.selectedIndex,
      chord: [[first[0], first[1]], [last[0], last[1]]],
      width: Math.min(placement.size[0], placement.size[1]),
      plane: createPlaneGrab(projection, links.handle) ?? null,
      planeHeight: links.handle[2],
      previous,
      last: previous ?? 0,
      hasSent: false,
    };
    return true;
  }

  function pointerMove(context: RoadBendSceneContext, input: RoadBendPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId) return true;
    const place = placeUnderPointer(context.engine, active, input);
    if (place === undefined) return true;
    const bend = bendThroughPoint(active.chord[0], active.chord[1], place, active.width);
    if (bend === active.last) return true;
    const result = context.engine.set_property(active.objectIndex, BEND_KEY, bend) as { ok?: boolean };
    if (result.ok === true) {
      active.last = bend;
      active.hasSent = true;
    }
    return true;
  }

  function pointerUp(context: RoadBendSceneContext, input: RoadBendPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId || (input.buttons & LEFT_BUTTON_BIT) !== 0) return true;
    gesture = null;
    if (active.last === (active.previous ?? 0)) {
      restore(context.engine, active);
      return true;
    }
    context.onCommitPlacement(active.objectIndex, [{ key: BEND_KEY, value: active.last, previous: active.previous }]);
    return true;
  }

  function pointerCancel(context: RoadBendSceneContext, input: RoadBendPointer): boolean {
    const active = gesture;
    if (active === null) return false;
    if (active.pointerId !== input.pointerId) return true;
    gesture = null;
    restore(context.engine, active);
    return true;
  }

  function cancel(context: RoadBendSceneContext): boolean {
    const active = gesture;
    if (active === null) return false;
    gesture = null;
    restore(context.engine, active);
    return true;
  }

  function drawHandle(context: RoadBendSceneContext, canvas: CanvasRenderingContext2D, links: RoadLinks, pointer: Vec2 | null, pixelRatio: number): void {
    if (!canBend(context)) return;
    const point = handleOnScreen(context.engine, links);
    if (point === undefined) return;
    const isUnderPointer = pointer !== null && distance(point, pointer) <= BEND_HANDLE_RADIUS_PX + HIT_MARGIN_PX;
    drawBendHandle(canvas, point, gesture !== null || isUnderPointer, pixelRatio);
  }

  return {
    start,
    pointerMove,
    pointerUp,
    pointerCancel,
    cancel,
    abandon: () => {
      gesture = null;
    },
    isActive: () => gesture !== null,
    drawHandle,
  };
}
