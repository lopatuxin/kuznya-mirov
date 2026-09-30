import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Path, path, smooth } from "./curve.mjs";

function near(a, b) {
  return Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9;
}

describe("smooth", () => {
  it("кривая проходит через каждую точку описания", () => {
    const points = [[0, 0], [10, 3], [14, 12], [30, 10]];
    const curve = smooth(points);
    for (const p of points) assert.ok(curve.some((q) => near(p, q)), `точка ${p}`);
    assert.ok(curve.length > points.length * 10, "кривая густая — через четверть клетки");
  });

  it("sharp — ломаная как есть", () => {
    const points = [[0, 0], [10, 3], [14, 12]];
    assert.deepEqual(smooth(points, { sharp: true }), points);
  });

  it("замкнутая кривая проходит через точки описания и возвращается к первой без угла", () => {
    const points = [[0, 0], [10, 0], [10, 10], [0, 10]];
    const curve = smooth(points, { closed: true });
    for (const p of points) assert.ok(curve.some((q) => near(p, q)), `точка ${p}`);
    const last = curve[curve.length - 1];
    assert.ok(Math.hypot(last[0], last[1]) < 0.5);
    const turn = (a, b, c2) => {
      const u = [b[0] - a[0], b[1] - a[1]];
      const v = [c2[0] - b[0], c2[1] - b[1]];
      return Math.acos((u[0] * v[0] + u[1] * v[1]) / (Math.hypot(...u) * Math.hypot(...v)));
    };
    const loop = [...curve, curve[0], curve[1]];
    for (let i = 0; i + 2 < loop.length; i++) assert.ok(turn(loop[i], loop[i + 1], loop[i + 2]) < 0.3, `излом у точки ${i + 1}`);
  });
});

describe("Path", () => {
  it("ближайшая точка, место вдоль линии и сторона: слева по ходу на восток — север", () => {
    const line = new Path([[0, 0], [10, 0]]);
    const north = line.nearest([4, -2]);
    assert.equal(north.distance, 2);
    assert.equal(north.s, 4);
    assert.equal(north.left, true);
    assert.equal(line.nearest([4, 3]).left, false);
  });

  it("место вдоль линии — от её начала", () => {
    const line = new Path([[0, 0], [10, 0], [10, 10]]);
    assert.equal(line.nearest([12, 5]).s, 15);
    assert.equal(line.length, 20);
  });

  it("острый излом: точка за вершиной — на стороне обоих отрезков", () => {
    const line = new Path([[0, 0], [10, 0], [0, 1]]);
    assert.equal(line.nearest([11, 0.5]).left, true);
    assert.equal(line.nearest([20, 0.5]).left, true);
    assert.equal(line.nearest([5, 0.3]).left, false, "внутри острого угла — справа по ходу обоих отрезков");
  });

  it("повтор соседней точки не даёт отрезка без направления", () => {
    const line = new Path([[0, 0], [5, 0], [5, 0], [10, 0]]);
    assert.equal(line.segments.length, 2);
    assert.equal(line.nearest([5, -1]).left, true);
  });

  it("многоугольник: внутри — минус расстояние до границы, снаружи — плюс", () => {
    const square = path([[0, 0], [10, 0], [10, 10], [0, 10]], { closed: true, sharp: true });
    assert.equal(square.signedDistance([5, 3]), -3);
    assert.equal(square.signedDistance([13, 5]), 3);
  });
});
