import { describe, expect, it } from "vitest";
import { drawRoadLinks, readRoadLinks, type RoadLinks } from "./roadLinks";

const RAW = {
  axis: [[0, 10, 0], [20, 10, 0]],
  links: [{ point: [10, 10, 0], road: 3 }],
  ends: [[0, 10, 0]],
  neighbours: [{ road: 3, axis: [[10, 0, 0], [10, 20, 0]] }],
};

describe("readRoadLinks", () => {
  it("читает ось, точки связей, тупики и оси соседей", () => {
    expect(readRoadLinks(RAW)).toEqual({
      axis: [[0, 10, 0], [20, 10, 0]],
      links: [[10, 10, 0]],
      ends: [[0, 10, 0]],
      neighbours: [[[10, 0, 0], [10, 20, 0]]],
    });
  });

  it("не дорога или игры нет — null", () => {
    expect(readRoadLinks(null)).toBeNull();
    expect(readRoadLinks(undefined)).toBeNull();
  });

  it("ответ без оси — null; негодные точки и соседи отбрасываются", () => {
    expect(readRoadLinks({ links: [], ends: [], neighbours: [] })).toBeNull();
    const links = readRoadLinks({ axis: RAW.axis, links: [{ road: 1 }], ends: ["x", [5, 5, 1]], neighbours: [{ road: 2 }] });
    expect(links).toEqual({ axis: RAW.axis, links: [], ends: [[5, 5, 1]], neighbours: [] });
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

  it("точка за камерой не рисуется", () => {
    const calls: Call[] = [];
    drawRoadLinks(recordingContext(calls), links, () => undefined, 1);

    expect(calls.map(([name]) => name)).not.toContain("lineTo");
    expect(calls.map(([name]) => name)).not.toContain("arc");
    expect(calls.map(([name]) => name)).not.toContain("rect");
  });
});
