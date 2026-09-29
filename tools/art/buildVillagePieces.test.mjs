import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { baseWidthPx, targetHeightFor } from "./buildVillagePieces.mjs";

// Картинка `width`×`height`, непрозрачная от `left` до `right` (включительно) в строке `y`, иначе
// прозрачная — имитирует силуэт дома без чтения настоящей картинки.
function rowsImage(width, height, rows) {
  const rgba = Buffer.alloc(width * height * 4);
  for (const { y, left, right } of rows) {
    for (let x = left; x <= right; x++) {
      const i = (y * width + x) * 4;
      rgba[i + 3] = 255;
    }
  }
  return rgba;
}

describe("baseWidthPx", () => {
  it("берёт ширину широкого цоколя, а не мелкой детали переднего плана под ним", () => {
    // Высота 100: нижняя четверть — строки 75..99, без последних 3% (97..99) — полоса 75..96.
    // Цоколь (широкий) — строки 75..90, деталь переднего плана (узкая) — строки 91..99.
    const rows = [];
    for (let y = 75; y <= 90; y++) rows.push({ y, left: 10, right: 89 }); // ширина 80
    for (let y = 91; y <= 99; y++) rows.push({ y, left: 40, right: 59 }); // ширина 20
    const rgba = rowsImage(100, 100, rows);
    assert.equal(baseWidthPx(rgba, 100, 100), 80);
  });

  it("не сбивается на пару строк резьбы/тени уже обычного внутри самого цоколя (медиана)", () => {
    const rows = [];
    for (let y = 75; y <= 96; y++) {
      const width = y === 80 ? 40 : 80; // одна узкая строка среди широких — выброс
      rows.push({ y, left: 10, right: 10 + width - 1 });
    }
    const rgba = rowsImage(100, 100, rows);
    assert.equal(baseWidthPx(rgba, 100, 100), 80);
  });

  it("падает, если в нижней четверти нет непрозрачных точек", () => {
    const rgba = Buffer.alloc(100 * 100 * 4);
    assert.throws(() => baseWidthPx(rgba, 100, 100), /основание не найдено/);
  });
});

describe("targetHeightFor", () => {
  it("без цели — общий множитель 0,8 от высоты", () => {
    assert.equal(targetHeightFor({ width: 100, height: 200 }, null), 160);
  });

  it("цель по ширине в клетках — картинка целиком масштабируется до неё, пропорции те же", () => {
    // Ширина 510 точек, цель 2 клетки = 192 точки: высота 419 -> 419 * 192 / 510 = 157,7 ≈ 158.
    assert.equal(targetHeightFor({ width: 510, height: 419 }, { axis: "width", cells: 2 }), 158);
  });

  it("цель по высоте в клетках — высота ровно цель × 96 точек", () => {
    assert.equal(targetHeightFor({ width: 283, height: 506 }, { axis: "height", cells: 4.2 }), 403);
  });
});
