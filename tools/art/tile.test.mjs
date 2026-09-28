import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { blendWithHalfShift, edgeWeight } from "./tile.mjs";

// Картинка `width` × `height`, где яркость точки — `value(x, y)`, во всех четырёх каналах.
function image(width, height, value) {
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) rgba.fill(value(x, y), (y * width + x) * 4, (y * width + x) * 4 + 4);
  }
  return rgba;
}

function at(rgba, width, x, y) {
  return rgba[(y * width + x) * 4];
}

describe("edgeWeight", () => {
  it("у обоих краёв ноль, в середине единица", () => {
    assert.equal(edgeWeight(0, 16), 0);
    assert.equal(edgeWeight(15, 16), 0);
    assert.equal(edgeWeight(8, 16), 1);
  });
});

describe("blendWithHalfShift", () => {
  it("по ширине крайние столбцы становятся соседними точками середины", () => {
    const width = 16;
    const out = blendWithHalfShift(image(width, 1, (x) => x * 10), width, 1, 0);
    assert.equal(at(out, width, 0, 0), 80);
    assert.equal(at(out, width, width - 1, 0), 70);
  });

  it("по высоте крайние строки становятся соседними точками середины, середина не меняется", () => {
    const height = 16;
    const out = blendWithHalfShift(image(1, height, (_, y) => y * 10), 1, height, 1);
    assert.equal(at(out, 1, 0, 0), 80);
    assert.equal(at(out, 1, 0, height - 1), 70);
    assert.equal(at(out, 1, 0, 8), 80);
  });
});
