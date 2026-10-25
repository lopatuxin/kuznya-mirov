import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { carve, glow } from "./carve.mjs";

// Камень 20 × 20 ровного серого цвета, буква — квадрат 6..13 по обеим осям.
const W = 20;
function stone() {
  return Buffer.alloc(W * W * 3, 180);
}
function letter() {
  const mask = Buffer.alloc(W * W);
  for (let y = 6; y < 14; y++) for (let x = 6; x < 14; x++) mask[y * W + x] = 255;
  return mask;
}
const grey = (rgb, x, y) => rgb[(y * W + x) * 3];

describe("carve", () => {
  const rgb = stone();
  carve(rgb, letter(), W, W, 1);

  it("камень вокруг буквы не тронут", () => {
    assert.equal(grey(rgb, 2, 2), 180);
    assert.equal(grey(rgb, 16, 10), 180);
  });

  it("внутри буквы камень темнее", () => {
    assert.ok(grey(rgb, 9, 10) < 120, `середина ${grey(rgb, 9, 10)}`);
  });

  it("стенка у правого верхнего края в тени, у левого нижнего — светлее середины", () => {
    assert.ok(grey(rgb, 13, 6) < grey(rgb, 9, 10), `тень ${grey(rgb, 13, 6)}`);
    assert.ok(grey(rgb, 6, 13) > grey(rgb, 9, 10), `свет ${grey(rgb, 6, 13)}`);
  });
});

describe("glow", () => {
  it("буква непрозрачна, ореол вокруг полупрозрачен, вдали пусто", () => {
    const box = { left: 0, top: 0, width: W, height: W };
    const rgba = glow(letter(), W, box, 10);
    const alpha = (x, y) => rgba[(y * W + x) * 4 + 3];
    assert.equal(alpha(9, 9), 255);
    assert.ok(alpha(15, 9) > 0 && alpha(15, 9) < 255, `ореол ${alpha(15, 9)}`);
    assert.equal(alpha(0, 0), 0);
  });
});
