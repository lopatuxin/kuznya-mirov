import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { path } from "./curve.mjs";
import { Grid, applyChannel, applyHill, applyNoise, applyPad, applyRange, applySmooth } from "./terrain.mjs";

function gcd(a, b) {
  return b === 0 ? a : gcd(b, a % b);
}

function heightAtPoint(grid, x, y) {
  return grid.at(Math.round(y * 2), Math.round(x * 2));
}

// Крутизна двух треугольников квадрата с левым верхним углом в (`row`, `col`), как у движка: тангенс.
function quadSlopes(grid, row, col) {
  const h00 = grid.at(row, col);
  const h10 = grid.at(row, col + 1);
  const h01 = grid.at(row + 1, col);
  const h11 = grid.at(row + 1, col + 1);
  return [Math.hypot(h10 - h00, h11 - h10) / 0.5, Math.hypot(h01 - h00, h11 - h01) / 0.5];
}

describe("Grid", () => {
  it("сетка — две ширины плюс одна точка в строке, две высоты плюс одна строка", () => {
    const grid = new Grid(4, 3);
    assert.equal(grid.cols, 9);
    assert.equal(grid.rows, 7);
    assert.equal(grid.h.length, 63);
  });

  it("высота между точками — по треугольникам с диагональю из левого верхнего угла", () => {
    const grid = new Grid(1, 1);
    grid.h[grid.cols + 1] = 2;
    assert.equal(grid.heightAt(0.25, 0.25), 1);
    assert.equal(grid.heightAt(0.5, 0.5), 2);
    assert.equal(grid.heightAt(0.5, 0), 0);
    assert.equal(grid.heightAt(-3, -3), 0, "за краем — высота края");
  });
});

describe("applyHill", () => {
  it("в середине — высота холма, за радиусом земля не тронута", () => {
    const grid = new Grid(20, 20);
    applyHill(grid, { at: [10, 10], radius: 4, height: 3, irregular: 0, seed: 1 });
    assert.equal(heightAtPoint(grid, 10, 10), 3);
    assert.equal(heightAtPoint(grid, 14, 10), 0);
    assert.equal(heightAtPoint(grid, 10, 15), 0);
    assert.ok(heightAtPoint(grid, 12, 10) > 0 && heightAtPoint(grid, 12, 10) < 3);
  });

  it("неровный холм: край то ближе, то дальше радиуса, но не дальше радиуса с долей irregular", () => {
    const grid = new Grid(40, 40);
    applyHill(grid, { at: [20, 20], radius: 8, height: 2, irregular: 0.4, seed: 9 });
    const edges = [];
    for (let k = 0; k < 32; k++) {
      const angle = (k / 32) * Math.PI * 2;
      let d = 0;
      while (d < 20 && grid.heightAt(20 + Math.cos(angle) * d, 20 + Math.sin(angle) * d) > 1e-9) d += 0.25;
      edges.push(d);
    }
    assert.ok(Math.max(...edges) - Math.min(...edges) > 2, `край холма ${Math.min(...edges)}…${Math.max(...edges)}`);
    assert.ok(Math.max(...edges) <= 8 * 1.4 + 0.5);
    assert.equal(heightAtPoint(grid, 20, 20), 2);
  });

  it("по любому лучу из середины неровный холм спадает до нуля один раз — без бугров за краем", () => {
    const grid = new Grid(40, 40);
    applyHill(grid, { at: [20, 20], radius: 8, height: 2, irregular: 0.9, seed: 5 });
    // Лучи через точки сетки: шаг (dc, dr) точек от середины — точка (40, 40) сетки.
    const steps = [];
    for (let dc = -3; dc <= 3; dc++) {
      for (let dr = -3; dr <= 3; dr++) if ((dc !== 0 || dr !== 0) && Math.abs(gcd(dc, dr)) === 1) steps.push([dc, dr]);
    }
    for (const [dc, dr] of steps) {
      let last = Infinity;
      for (let k = 0; Math.abs(40 + k * dr) <= 80 && 40 + k * dr >= 0 && 40 + k * dc >= 0 && 40 + k * dc <= 80; k++) {
        const h = grid.at(40 + k * dr, 40 + k * dc);
        assert.ok(h <= last + 1e-12, `луч (${dc}, ${dr}): высота растёт на шаге ${k}`);
        last = h;
      }
    }
  });

  it("отрицательная высота — яма", () => {
    const grid = new Grid(10, 10);
    applyHill(grid, { at: [5, 5], radius: 3, height: -2, irregular: 0, seed: 1 });
    assert.equal(heightAtPoint(grid, 5, 5), -2);
  });
});

describe("applyPad", () => {
  it("внутри — высота площадки, за кромкой земля не тронута", () => {
    const grid = new Grid(20, 20);
    applyHill(grid, { at: [10, 10], radius: 8, height: 4, irregular: 0, seed: 1 });
    const before = heightAtPoint(grid, 19, 10);
    applyPad(grid, { area: path([[8, 8], [12, 8], [12, 12], [8, 12]], { closed: true, sharp: true }), height: 1.5, rim: 2 });
    assert.equal(heightAtPoint(grid, 10, 10), 1.5);
    assert.equal(heightAtPoint(grid, 8.5, 11.5), 1.5);
    assert.equal(heightAtPoint(grid, 19, 10), before);
  });

  it("без высоты — средняя высота земли под площадкой", () => {
    const grid = new Grid(10, 10);
    grid.each(grid.whole, (index, x) => {
      grid.h[index] = x;
    });
    // Углы между точками сетки: внутрь попадают столбцы от 2,5 до 5,5, их средняя — 4.
    applyPad(grid, { area: path([[2.2, 2.2], [5.8, 2.2], [5.8, 5.8], [2.2, 5.8]], { closed: true, sharp: true }), rim: 1 });
    assert.ok(Math.abs(heightAtPoint(grid, 4, 4) - 4) < 1e-9);
    assert.ok(Math.abs(heightAtPoint(grid, 2.5, 5) - 4) < 1e-9);
  });
});

describe("applyChannel", () => {
  it("на оси русла — высота дна, берег сходит к земле на своей ширине, земля не поднимается", () => {
    const grid = new Grid(20, 10);
    grid.h.fill(1);
    applyHill(grid, { at: [3, 5], radius: 2, height: -5, irregular: 0, seed: 1 });
    applyChannel(grid, { line: path([[0, 5], [20, 5]]), width: 2, bottom: -1, bank: 2 });
    assert.equal(heightAtPoint(grid, 10, 5), -1);
    assert.equal(heightAtPoint(grid, 10, 6), -1, "край дна");
    const bank = heightAtPoint(grid, 10, 7);
    assert.ok(bank > -1 && bank < 1, `берег ${bank}`);
    assert.equal(heightAtPoint(grid, 10, 8), 1, "за берегом");
    assert.equal(heightAtPoint(grid, 3, 5), -4, "яма глубже дна осталась ямой");
  });

  it("дно от первой высоты в начале линии до второй в конце", () => {
    const grid = new Grid(20, 10);
    grid.h.fill(5);
    applyChannel(grid, { line: path([[0, 5], [20, 5]]), width: 1, bottom: [0, 4], bank: 1 });
    assert.equal(heightAtPoint(grid, 0, 5), 0);
    assert.ok(Math.abs(heightAtPoint(grid, 10, 5) - 2) < 1e-9);
    assert.ok(Math.abs(heightAtPoint(grid, 20, 5) - 4) < 1e-9);
  });
});

describe("applyRange", () => {
  const foot = () => path([[-5, 20], [45, 20]]);

  it("в сторону гор земля выше, по другую сторону линии не тронута", () => {
    const grid = new Grid(40, 40);
    applyRange(grid, { foot: foot(), side: "left", height: 10, depth: 15, roughness: 0.3, wavelength: 10, warp: 0, seed: 1 });
    for (let x = 0; x <= 40; x += 0.5) {
      assert.equal(heightAtPoint(grid, x, 25), 0);
      assert.ok(heightAtPoint(grid, x, 10) > heightAtPoint(grid, x, 18));
      // Распадки забирают долю roughness, массивы — ещё до 0,35 × roughness высоты.
      assert.ok(heightAtPoint(grid, x, 2) >= 10 * 0.7 * (1 - 0.35 * 0.3) - 1e-9, "за глубиной не ниже доли без хребтов");
    }
  });

  it("без roughness за глубиной — ровно height, с ней — острые гребни и распадки", () => {
    const flat = new Grid(40, 40);
    applyRange(flat, { foot: foot(), side: "left", height: 10, depth: 15, roughness: 0, wavelength: 10, warp: 0, seed: 1 });
    assert.ok(Math.abs(heightAtPoint(flat, 20, 2) - 10) < 1e-9);
    const rough = new Grid(40, 40);
    applyRange(rough, { foot: foot(), side: "left", height: 10, depth: 15, roughness: 0.8, wavelength: 8, warp: 0, seed: 1 });
    const crest = Array.from({ length: 81 }, (_, i) => heightAtPoint(rough, i / 2, 2));
    assert.ok(Math.max(...crest) - Math.min(...crest) > 5, `гребень от ${Math.min(...crest)} до ${Math.max(...crest)}`);
  });

  it("left — север, если линия идёт на восток; right — юг", () => {
    const grid = new Grid(40, 40);
    applyRange(grid, { foot: foot(), side: "right", height: 10, depth: 15, roughness: 0, wavelength: 10, warp: 0, seed: 1 });
    assert.equal(heightAtPoint(grid, 20, 10), 0);
    assert.ok(heightAtPoint(grid, 20, 30) > 0);
  });

  it("warp уводит подножие от линии в обе стороны, но не дальше warp", () => {
    const grid = new Grid(40, 40);
    applyRange(grid, { foot: foot(), side: "left", height: 10, depth: 15, roughness: 0, wavelength: 6, warp: 3, seed: 4 });
    const footAt = [];
    for (let x = 0; x <= 40; x += 0.5) {
      let y = 40;
      while (y > 0 && grid.heightAt(x, y) <= 1e-9) y -= 0.5;
      footAt.push(y);
    }
    assert.ok(Math.max(...footAt) > 20.5, "отроги выходят за линию");
    assert.ok(Math.min(...footAt) < 19.5, "заливы уходят за линию в горы");
    assert.ok(Math.max(...footAt) <= 23 && Math.min(...footAt) >= 16.5);
  });

  it("у подножия склон круче 45°", () => {
    const grid = new Grid(40, 40);
    applyRange(grid, { foot: foot(), side: "left", height: 10, depth: 15, roughness: 0.4, wavelength: 10, warp: 0, seed: 3 });
    for (let col = 0; col < grid.cols - 1; col++) {
      const steepest = Math.max(...quadSlopes(grid, 38, col), ...quadSlopes(grid, 39, col));
      assert.ok(steepest > 1, `у подножия в столбце ${col} крутизна ${steepest}`);
    }
  });
});

describe("applyNoise", () => {
  it("одно зерно — одни неровности, другое — другие; в области гаснут к краю", () => {
    const area = path([[5, 5], [35, 5], [35, 35], [5, 35]], { closed: true, sharp: true });
    const make = (seed) => {
      const grid = new Grid(40, 40);
      applyNoise(grid, { amplitude: 1, wavelength: 6, seed, area, fade: 4 });
      return grid;
    };
    assert.deepEqual(make(7).h, make(7).h);
    assert.notDeepEqual(make(7).h, make(8).h);
    const grid = make(7);
    assert.equal(heightAtPoint(grid, 5, 20), 0, "на краю области");
    assert.equal(heightAtPoint(grid, 2, 20), 0, "за областью");
    assert.ok(grid.h.some((h) => Math.abs(h) > 0.1));
  });
});

describe("applySmooth", () => {
  it("острый пик становится средним соседей", () => {
    const grid = new Grid(4, 4);
    grid.h[4 * grid.cols + 4] = 9;
    applySmooth(grid, { passes: 1 });
    assert.equal(heightAtPoint(grid, 2, 2), 1);
    assert.equal(heightAtPoint(grid, 2.5, 2.5), 1);
    assert.equal(heightAtPoint(grid, 3, 3), 0);
  });
});
