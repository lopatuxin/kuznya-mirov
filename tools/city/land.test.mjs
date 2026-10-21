import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { buildLand, distanceField } from "./land.mjs";

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

/** Точки ниже воды, до которых от моря не дойти по воде: в движке они стали бы прудами. */
function lonePonds(map) {
  const at = (c, r) => map.heights[r * map.cols + c];
  const seen = new Uint8Array(map.cols * map.rows);
  const queue = [];
  for (let c = 0; c < map.cols; c++) if (at(c, 0) < 0) {
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
      if (!seen[j] && map.heights[j] < 0) {
        seen[j] = 1;
        queue.push(j);
      }
    }
  }
  let lone = 0;
  for (let i = 0; i < map.heights.length; i++) if (map.heights[i] < 0 && !seen[i]) lone++;
  return lone;
}

test("карта: вся вода сходится к морю — ни луж, ни озёр без выхода", () => {
  assert.equal(lonePonds(land), 0);
});

test("карта игры: на настоящих настройках вода тоже сходится к морю", () => {
  assert.equal(lonePonds(buildLand(full)), 0);
});

test("карта: по руслу каждой реки — вода", () => {
  const spacing = land.spacing;
  for (const river of land.rivers) {
    for (const p of river.points) {
      const c = Math.round(p.x / spacing), r = Math.round(p.y / spacing);
      assert.ok(at(c, r) < 0, `суша на русле в ${Math.round(p.x)}, ${Math.round(p.y)} м`);
    }
  }
});

test("расстояние до отмеченной точки — точное, по прямой", () => {
  const mask = new Uint8Array(25);
  mask[0] = 1;
  const d = distanceField(mask, 5, 5, 10);
  assert.equal(d[0], 0);
  assert.equal(d[4], 40);
  assert.ok(Math.abs(d[24] - Math.hypot(40, 40)) < 1e-6);
});
