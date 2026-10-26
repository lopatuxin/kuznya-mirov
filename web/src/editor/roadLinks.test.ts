import { describe, expect, it } from "vitest";
import { BEND_HANDLE_RADIUS_PX, drawBendHandle, drawRoadLinks, readRoadLinks, type RoadLinks } from "./roadLinks";

const RAW = {
  axis: [[0, 10, 0], [20, 10, 0]],
  bend: 0,
  handle: [10, 10, 0],
  links: [{ point: [10, 10, 0], road: 3 }],
  ends: [[0, 10, 0]],
  neighbours: [{ road: 3, axis: [[10, 0, 0], [10, 20, 0]], bend: 0 }],
};

/** Дуга из пяти точек с высотой профиля в каждой: подъём от 0 до 1. */
const ARC = [[0, 10, 0], [5, 8, 0.25], [10, 7.5, 0.5], [15, 8, 0.75], [20, 10, 1]];

describe("readRoadLinks", () => {
  it("читает ось, изгиб, ручку, точки связей, тупики и оси соседей", () => {
    expect(readRoadLinks(RAW)).toEqual({
      axis: [[0, 10, 0], [20, 10, 0]],
      bend: 0,
      handle: [10, 10, 0],
      links: [[10, 10, 0]],
      ends: [[0, 10, 0]],
      neighbours: [[[10, 0, 0], [10, 20, 0]]],
    });
  });

  it("ось дугой — ломаная со своей высотой в каждой точке; изгиб и ручка берутся как есть", () => {
    const links = readRoadLinks({ ...RAW, axis: ARC, bend: -42.5, handle: [10, 7.5, 0.5], neighbours: [{ road: 4, axis: ARC, bend: 10 }] });

    expect(links?.axis).toEqual(ARC);
    expect(links?.bend).toBe(-42.5);
    expect(links?.handle).toEqual([10, 7.5, 0.5]);
    expect(links?.neighbours).toEqual([ARC]);
  });

  it("не дорога или игры нет — null", () => {
    expect(readRoadLinks(null)).toBeNull();
    expect(readRoadLinks(undefined)).toBeNull();
  });

  it("ответ без оси, изгиба или ручки — null; ось из одной точки или с негодной точкой — тоже", () => {
    expect(readRoadLinks({ ...RAW, axis: undefined })).toBeNull();
    expect(readRoadLinks({ ...RAW, axis: [[0, 10, 0]] })).toBeNull();
    expect(readRoadLinks({ ...RAW, axis: [[0, 10, 0], "x", [20, 10, 0]] })).toBeNull();
    expect(readRoadLinks({ ...RAW, bend: undefined })).toBeNull();
    expect(readRoadLinks({ ...RAW, bend: Number.NaN })).toBeNull();
    expect(readRoadLinks({ ...RAW, handle: undefined })).toBeNull();
  });

  it("негодные точки связей, тупики и соседи отбрасываются", () => {
    const links = readRoadLinks({ ...RAW, links: [{ road: 1 }], ends: ["x", [5, 5, 1]], neighbours: [{ road: 2 }, { road: 3, axis: [[1, 1, 0]] }] });
    expect(links).toMatchObject({ links: [], ends: [[5, 5, 1]], neighbours: [] });
  });
});

type Call = [string, ...unknown[]];

/** Холст, что записывает вызовы рисования; у `stroke` — ещё толщину линии в этот миг. */
function recordingContext(calls: Call[]): CanvasRenderingContext2D {
  const state: Record<string, unknown> = {};
  return new Proxy(state, {
    get: (record, name: string) => (name in record ? record[name] : (...args: unknown[]) => calls.push(name === "stroke" ? [name, record.lineWidth] : [name, ...args])),
    set: (record, name: string, value: unknown) => {
      record[name] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

describe("drawRoadLinks", () => {
  const links = readRoadLinks(RAW) as RoadLinks;
  /** Сверху без перспективы: 10 точек на клетку. */
  const project = (point: readonly [number, number, number]): [number, number] => [point[0] * 10, point[1] * 10];

  it("рисует оси, квадратик тупика и кружок связи; толщина линии одна при любом приближении", () => {
    const calls: Call[] = [];
    drawRoadLinks(recordingContext(calls), links, project, 1);
    const names = calls.map(([name]) => name);

    // Ось и ось соседа, каждая дважды: тёмная кайма и светлая линия.
    expect(calls.filter(([name]) => name === "lineTo")).toHaveLength(4);
    expect(names).toContain("rect");
    expect(names).toContain("arc");
    expect(calls.find(([name]) => name === "rect")).toEqual(["rect", -4, 96, 8, 8]);
    const widths = new Set(calls.filter(([name]) => name === "stroke").map(([, width]) => width));
    expect(widths).toEqual(new Set([3, 1]));
  });

  it("ось дугой — ломаная через каждую точку, высота точки уходит в проекцию", () => {
    const calls: Call[] = [];
    const projected: number[] = [];
    drawRoadLinks(
      recordingContext(calls),
      { ...links, axis: ARC.map((point) => point as [number, number, number]), neighbours: [] },
      (point) => {
        projected.push(point[2]);
        return project(point);
      },
      1,
    );

    // Пять точек оси: moveTo в первую и lineTo в остальные четыре, дважды — кайма и линия.
    expect(calls.filter(([name]) => name === "moveTo")).toHaveLength(2);
    expect(calls.filter(([name]) => name === "lineTo")).toHaveLength(8);
    expect(projected).toEqual(expect.arrayContaining([0.25, 0.5, 0.75, 1]));
  });

  it("точка оси за камерой разрывает линию: дальше она начинается заново", () => {
    const calls: Call[] = [];
    const behind = (point: readonly [number, number, number]): [number, number] | undefined => (point[0] === 10 ? undefined : project(point));
    drawRoadLinks(recordingContext(calls), { ...links, axis: ARC.map((point) => point as [number, number, number]), neighbours: [] }, behind, 1);

    // Две видимые части по две точки: два moveTo и два lineTo на каждую из двух обводок.
    expect(calls.filter(([name]) => name === "moveTo")).toHaveLength(4);
    expect(calls.filter(([name]) => name === "lineTo")).toHaveLength(4);
  });

  it("точка за камерой не рисуется", () => {
    const calls: Call[] = [];
    drawRoadLinks(recordingContext(calls), links, () => undefined, 1);

    expect(calls.map(([name]) => name)).not.toContain("lineTo");
    expect(calls.map(([name]) => name)).not.toContain("arc");
    expect(calls.map(([name]) => name)).not.toContain("rect");
  });
});

describe("drawBendHandle", () => {
  it("круг радиуса ручки в точке холста; светлеет, когда на него указывают", () => {
    const calls: Call[] = [];
    const fills: unknown[] = [];
    const context = recordingContext(calls);
    const spy = new Proxy(context, {
      get: (target, name: string) => (name === "fill" ? () => fills.push((target as unknown as Record<string, unknown>).fillStyle) : Reflect.get(target, name)),
    });

    drawBendHandle(spy, [120, 80], false, 1);
    drawBendHandle(spy, [120, 80], true, 1);

    expect(calls.filter(([name]) => name === "arc")).toEqual([
      ["arc", 120, 80, BEND_HANDLE_RADIUS_PX, 0, Math.PI * 2],
      ["arc", 120, 80, BEND_HANDLE_RADIUS_PX, 0, Math.PI * 2],
    ]);
    expect(fills).toEqual(["#ffc83d", "#ffffff"]);
  });
});
