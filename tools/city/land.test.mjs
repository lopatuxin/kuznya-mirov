import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { buildLand, distanceField, landWater } from "./land.mjs";

const full = JSON.parse(readFileSync(new URL("../../art/transport/land.json", import.meta.url), "utf8"));
const small = { ...full, size_m: [3000, 3000], sea: { ...full.sea, width_m: 500, coast_wiggle_m: 150 } };
const land = buildLand(small);
const at = (c, r) => land.heights[r * land.cols + c];

test("карта: одно зерно — та же карта, другое — другая", () => {
  assert.deepEqual(buildLand(small).heights, land.heights);
  assert.notDeepEqual(buildLand({ ...small, seed: small.seed + 1 }).heights, land.heights);
});

test("карта: точек по сторонам — клетки × густота + 1", () => {
  assert.equal(land.cols, (3000 / small.cell_m) * small.points_per_cell + 1);
  assert.equal(land.rows, land.cols);
});

test("карта: север — море, юг — суша", () => {
  for (let c = 0; c < land.cols; c++) {
    assert.ok(at(c, 0) < 0, `у северного края суша в точке ${c}`);
    assert.ok(at(c, land.rows - 1) > 0, `у южного края вода в точке ${c}`);
  }
});

test("карта: реки легли все, со своими притоками", () => {
  const expected = small.rivers.reduce((sum, r) => sum + 1 + r.tributaries.length, 0);
  assert.equal(land.rivers.length, expected);
  for (const river of land.rivers) assert.ok(river.length > 0);
});

/**
 * Вода по правилам движка («Рельеф», «Вода»): русла рек, потом озёра заливкой, не проходя русла с гладью ниже, потом
 * море — в метрах. `owner`: −1 суша, 0 море, 1 + номер озера, 100 + номер реки.
 */
function engineWater(map, config) {
  const water = landWater(map, config);
  const m = (v) => v * config.cell_m;
  const n = map.cols * map.rows;
  const owner = new Int32Array(n).fill(-1);
  const surface = new Float64Array(n);
  const stripRiver = new Int32Array(n).fill(-1);
  const stripS = new Float64Array(n);
  for (const [ri, river] of water.rivers.entries()) {
    const best = new Float64Array(n).fill(Infinity), bestS = new Float64Array(n), bestW = new Float64Array(n);
    const pts = river.points.map((p) => p.map(m));
    for (let k = 0; k + 1 < pts.length; k++) {
      const [ax, ay, ah, aw] = pts[k], [bx, by, bh, bw] = pts[k + 1];
      const reach = Math.max(aw, bw) / 2 + map.spacing;
      const c0 = Math.max(0, Math.floor((Math.min(ax, bx) - reach) / map.spacing)), c1 = Math.min(map.cols - 1, Math.ceil((Math.max(ax, bx) + reach) / map.spacing));
      const r0 = Math.max(0, Math.floor((Math.min(ay, by) - reach) / map.spacing)), r1 = Math.min(map.rows - 1, Math.ceil((Math.max(ay, by) + reach) / map.spacing));
      const ux = bx - ax, uy = by - ay, uu = ux * ux + uy * uy;
      for (let r = r0; r <= r1; r++) {
        for (let c = c0; c <= c1; c++) {
          const x = c * map.spacing, y = r * map.spacing, i = r * map.cols + c;
          const t = uu > 0 ? Math.max(0, Math.min(1, ((x - ax) * ux + (y - ay) * uy) / uu)) : 0;
          const d = Math.hypot(x - ax - t * ux, y - ay - t * uy);
          if (d < best[i]) {
            best[i] = d;
            bestS[i] = ah + (bh - ah) * t;
            bestW[i] = aw + (bw - aw) * t;
          }
        }
      }
    }
    for (let i = 0; i < n; i++) {
      if (stripRiver[i] >= 0 || !(best[i] <= bestW[i] / 2)) continue;
      stripRiver[i] = ri;
      stripS[i] = bestS[i];
      if (map.heights[i] < bestS[i]) {
        owner[i] = 100 + ri;
        surface[i] = bestS[i];
      }
    }
  }
  const lakeEdge = [];
  for (const [li, lake] of water.lakes.entries()) {
    const level = m(lake.level);
    const startAt = Math.round(m(lake.at[1]) / map.spacing) * map.cols + Math.round(m(lake.at[0]) / map.spacing);
    const seen = new Uint8Array(n);
    const queue = [startAt];
    seen[startAt] = 1;
    let edge = false;
    while (queue.length) {
      const i = queue.pop();
      if (!(map.heights[i] < level)) continue;
      if (stripRiver[i] >= 0) {
        if (stripS[i] < level - m(0.001)) continue;
      } else if (owner[i] < 0) {
        owner[i] = 1 + li;
        surface[i] = level;
      } else continue;
      const c = i % map.cols, r = (i - c) / map.cols;
      if (c === 0 || r === 0 || c === map.cols - 1 || r === map.rows - 1) edge = true;
      for (const [dc, dr] of [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, -1]]) {
        const nc = c + dc, nr = r + dr;
        if (nc < 0 || nr < 0 || nc >= map.cols || nr >= map.rows) continue;
        const j = nr * map.cols + nc;
        if (!seen[j]) {
          seen[j] = 1;
          queue.push(j);
        }
      }
    }
    lakeEdge.push(edge);
  }
  for (let i = 0; i < n; i++) {
    if (stripRiver[i] < 0 && owner[i] < 0 && map.heights[i] < m(water.level)) {
      owner[i] = 0;
      surface[i] = m(water.level);
    }
  }
  return { water, owner, surface, stripRiver, lakeEdge };
}

const engine = engineWater(land, small);
const fullLand = buildLand(full);
const fullEngine = engineWater(fullLand, full);

for (const [name, map, result] of [["карта", land, engine], ["карта игры", fullLand, fullEngine]]) {
  test(`${name}: гладь каждой реки к устью не поднимается`, () => {
    for (const [ri, river] of result.water.rivers.entries()) {
      for (let k = 1; k < river.points.length; k++) {
        assert.ok(river.points[k][2] <= river.points[k - 1][2], `река ${ri + 1}, точка ${k + 1} выше предыдущей`);
      }
    }
  });

  test(`${name}: по линии каждой реки — вода`, () => {
    for (const [ri, river] of result.water.rivers.entries()) {
      for (const [k, p] of river.points.entries()) {
        // Точка линии лежит между точками сетки: вода должна быть хотя бы в одной из четырёх соседних — у устья
        // ближайшая может оказаться уже на пляже.
        const cell = (name === "карта" ? small : full).cell_m;
        const gx = (p[0] * cell) / map.spacing, gy = (p[1] * cell) / map.spacing;
        const around = [[0, 0], [1, 0], [0, 1], [1, 1]].map(([dc, dr]) => (Math.floor(gy) + dr) * map.cols + Math.floor(gx) + dc);
        assert.ok(around.some((i) => result.owner[i] >= 0), `река ${ri + 1}: суша на линии у точки ${k + 1}`);
      }
    }
  });

  test(`${name}: вода реки не упирается в край русла — за ним земля выше глади`, () => {
    let cut = 0;
    for (let i = 0; i < map.heights.length; i++) {
      if (result.owner[i] < 100) continue;
      const c = i % map.cols, r = (i - c) / map.cols;
      for (const [dc, dr] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const j = (r + dr) * map.cols + c + dc;
        if (result.stripRiver[j] < 0 && result.owner[j] < 0 && map.heights[j] < result.surface[i]) cut++;
      }
    }
    assert.equal(cut, 0);
  });

  test(`${name}: озеро заливает свою чашу и не уходит к краю карты`, () => {
    for (const [li, lake] of result.water.lakes.entries()) {
      let count = 0;
      for (const o of result.owner) if (o === 1 + li) count++;
      assert.ok(count > 0, `озеро ${li + 1} пустое`);
      assert.equal(result.lakeEdge[li], false, `озеро ${li + 1} разлилось до края`);
      const spec = (name === "карта" ? small : full).rivers.find((r) => r.lake).lake;
      assert.ok(count * map.spacing ** 2 < Math.PI * spec.radius_m ** 2, `озеро ${li + 1} шире своей чаши`);
    }
  });

  test(`${name}: река, впадающая в море, кончается на его уровне`, () => {
    const config = name === "карта" ? small : full;
    let atSea = 0;
    for (const [ri, river] of result.water.rivers.entries()) {
      const end = river.points[river.points.length - 1];
      const [x, y] = [end[0] * config.cell_m, end[1] * config.cell_m];
      if (y >= map.coast(x) + 2 * config.cell_m) continue;
      atSea++;
      assert.equal(end[2], 0, `река ${ri + 1} у моря выше его уровня`);
    }
    assert.ok(atSea > 0, "ни одна река не дошла до моря");
  });

  test(`${name}: середина реки в озере залита самим озером — течения через озеро нет`, () => {
    for (const river of map.rivers.filter((r) => r.lake)) {
      const { outlet, inlet } = river.lake;
      const span = inlet - outlet;
      for (const p of river.points.slice(outlet + Math.ceil(span / 3), inlet - Math.ceil(span / 3) + 1)) {
        const c = Math.round(p.x / map.spacing), r = Math.round(p.y / map.spacing);
        assert.ok(result.owner[r * map.cols + c] >= 1 && result.owner[r * map.cols + c] < 100, `в ${Math.round(p.x)}, ${Math.round(p.y)} м не озеро`);
      }
    }
  });

  test(`${name}: ниже моря только море, русла и озёра — луж нет`, () => {
    const seen = new Uint8Array(map.cols * map.rows);
    const queue = [];
    for (let c = 0; c < map.cols; c++) if (result.owner[c] === 0) {
      seen[c] = 1;
      queue.push(c);
    }
    while (queue.length) {
      const i = queue.pop();
      const c = i % map.cols, r = (i - c) / map.cols;
      for (const [dc, dr] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const nc = c + dc, nr = r + dr;
        if (nc < 0 || nr < 0 || nc >= map.cols || nr >= map.rows) continue;
        const j = nr * map.cols + nc;
        if (!seen[j] && result.owner[j] === 0) {
          seen[j] = 1;
          queue.push(j);
        }
      }
    }
    let lone = 0;
    for (let i = 0; i < seen.length; i++) if (result.owner[i] === 0 && !seen[i]) lone++;
    assert.equal(lone, 0);
  });
}

test("расстояние до отмеченной точки — точное, по прямой", () => {
  const mask = new Uint8Array(25);
  mask[0] = 1;
  const d = distanceField(mask, 5, 5, 10);
  assert.equal(d[0], 0);
  assert.equal(d[4], 40);
  assert.ok(Math.abs(d[24] - Math.hypot(40, 40)) < 1e-6);
});
