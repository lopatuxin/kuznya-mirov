import { describe, expect, it } from "vitest";
import { drawSceneOutlines, outlineIdAtPoint, readSceneOutlines, type SceneOutline, type SceneOutlinesEngine } from "./sceneOutlines";

const RECT = { x: 100, y: 200, width: 80, height: 40 };

describe("readSceneOutlines", () => {
  it("собирает вид у движка, а прямоугольник на холсте — по номеру; объект без прямоугольника пропускает", () => {
    const rects: Record<number, typeof RECT> = { 3: RECT, 5: { x: 0, y: 0, width: 10, height: 10 } };
    const engine = {
      outlines: () => [
        { id: 3, kind: "solid" },
        { id: 4, kind: "other" },
        { id: 5, kind: "one_way" },
      ],
      object_rect: (id: number) => rects[id],
    } as unknown as SceneOutlinesEngine;

    expect(readSceneOutlines(engine)).toEqual([
      { id: 3, kind: "solid", rect: RECT },
      { id: 5, kind: "one_way", rect: rects[5] },
    ]);
  });
});

describe("outlineIdAtPoint", () => {
  const outlines: SceneOutline[] = [{ id: 1, kind: "solid", rect: RECT }];

  it("попадает не дальше 6 точек от линии — снаружи и внутри", () => {
    expect(outlineIdAtPoint(outlines, [100 - 6, 220])).toBe(1);
    expect(outlineIdAtPoint(outlines, [100 + 6, 220])).toBe(1);
    expect(outlineIdAtPoint(outlines, [140, 200 - 6])).toBe(1);
    expect(outlineIdAtPoint(outlines, [140, 240 - 6])).toBe(1);
  });

  it("дальше 6 точек от линии не попадает: ни снаружи, ни внутри, ни у угла по диагонали", () => {
    expect(outlineIdAtPoint(outlines, [100 - 7, 220])).toBeUndefined();
    expect(outlineIdAtPoint(outlines, [140, 220])).toBeUndefined();
    expect(outlineIdAtPoint(outlines, [100 - 5, 200 - 5])).toBeUndefined();
  });

  it("из нескольких берёт ближнюю линию, из равных — нарисованную позже", () => {
    const near: SceneOutline = { id: 2, kind: "other", rect: { x: 103, y: 200, width: 80, height: 40 } };
    const same: SceneOutline = { id: 3, kind: "other", rect: RECT };

    expect(outlineIdAtPoint([...outlines, near], [102, 220])).toBe(2);
    expect(outlineIdAtPoint([...outlines, same], [102, 220])).toBe(3);
  });

  it("список пуст — никто не выбран", () => {
    expect(outlineIdAtPoint([], [100, 200])).toBeUndefined();
  });
});

describe("drawSceneOutlines", () => {
  type Call = [string, ...unknown[]];

  function recordingContext(calls: Call[]): CanvasRenderingContext2D {
    return new Proxy({}, { get: (_target, property: string) => (...args: unknown[]) => calls.push([property, ...args]) }) as unknown as CanvasRenderingContext2D;
  }

  it("без прямоугольников ничего не рисует", () => {
    const calls: Call[] = [];

    drawSceneOutlines(recordingContext(calls), [], 2);

    expect(calls).toEqual([]);
  });

  it("твёрдое — заливка и один сплошной прямоугольник под каймой", () => {
    const calls: Call[] = [];

    drawSceneOutlines(recordingContext(calls), [{ id: 1, kind: "solid", rect: RECT }], 1);

    expect(calls.filter(([name]) => name === "fillRect")).toEqual([["fillRect", 100, 200, 80, 40]]);
    expect(calls.filter(([name]) => name === "setLineDash").map(([, dash]) => dash)).toEqual([[]]);
  });

  it("площадка — сплошная линия сверху и пунктир по трём остальным сторонам, без заливки", () => {
    const calls: Call[] = [];

    drawSceneOutlines(recordingContext(calls), [{ id: 1, kind: "one_way", rect: RECT }], 1);

    expect(calls.some(([name]) => name === "fillRect")).toBe(false);
    expect(calls.filter(([name]) => name === "setLineDash").map(([, dash]) => dash)).toEqual([[6, 4], []]);
    expect(calls.filter(([name]) => name === "lineTo")).toContainEqual(["lineTo", 180, 200]);
  });

  it("прочее — тонкий пунктир без заливки", () => {
    const calls: Call[] = [];

    drawSceneOutlines(recordingContext(calls), [{ id: 1, kind: "other", rect: RECT }], 1);

    expect(calls.some(([name]) => name === "fillRect")).toBe(false);
    expect(calls.filter(([name]) => name === "setLineDash").map(([, dash]) => dash)).toEqual([[3, 3]]);
  });
});
