import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { composeOverShadow, figureRows, shadowDensity } from "./shadows.mjs";

// Кадр 100 × 40 с фигурой-столбиком шириной 4 от строки 10 до строки 29: тень уходит влево.
const W = 100;
const H = 40;
function column() {
  const alpha = new Uint8Array(W * H);
  for (let y = 10; y <= 29; y++) for (let x = 70; x < 74; x++) alpha[y * W + x] = 255;
  return alpha;
}

describe("figureRows", () => {
  it("находит верх и низ сплошной фигуры", () => {
    assert.deepEqual(figureRows(column(), W, H), { top: 10, bottom: 29 });
  });

  it("без сплошных точек фигуры нет", () => {
    assert.equal(figureRows(new Uint8Array(W * H).fill(100), W, H), null);
  });
});

describe("shadowDensity", () => {
  const density = shadowDensity(column(), W, H, 1);
  const at = (x, y) => density[y * W + x];

  it("под ступнёй тень плотная, над головой её нет", () => {
    assert.ok(at(71, 31) > 0.5, `под ступнёй ${at(71, 31)}`);
    assert.equal(at(71, 2), 0);
  });

  it("тень фигуры уходит влево и чуть вниз от ступни: верх фигуры ложится на 0,55 её высоты левее", () => {
    // Верх на 19 строк выше ступни: тень — на 10 точек левее и на 4 строки ниже ступни.
    assert.ok(at(61, 33) > 0.3, `тень верха ${at(61, 33)}`);
    assert.ok(at(95, 33) < 0.05, `справа тени нет: ${at(95, 33)}`);
  });

  it("без фигуры тени нет", () => {
    assert.ok(shadowDensity(new Uint8Array(W * H), W, H, 1).every((value) => value === 0));
  });
});

describe("composeOverShadow", () => {
  it("герой закрывает тень, а пустая точка получает цвет и плотность тени", () => {
    const rgba = Buffer.from([200, 100, 50, 255, 0, 0, 0, 0]);
    const out = composeOverShadow(rgba, Float32Array.from([0.8, 0.5]));
    assert.deepEqual([...out.subarray(0, 4)], [200, 100, 50, 255]);
    assert.deepEqual([...out.subarray(4, 8)], [0x2a, 0x1a, 0x0e, 128]);
  });
});
