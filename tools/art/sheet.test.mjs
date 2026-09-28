import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { findCycle, keyGreen, parseRange, pickFrames } from "./sheet.mjs";

const THUMB = 64;

// Эскиз кадра — ровная заливка яркостью `level`: разность двух эскизов равна разности яркостей.
function flat(level) {
  return new Uint8Array(THUMB).fill(level);
}

describe("parseRange", () => {
  it("переводит кадры A-B, считая с единицы, в начало и длину, считая с нуля", () => {
    assert.deepEqual(parseRange("3-5"), { start: 2, length: 3 });
    assert.deepEqual(parseRange("7-7"), { start: 6, length: 1 });
  });

  it("отвергает нулевой кадр, обратный порядок и не число", () => {
    for (const text of ["0-4", "5-3", "abc", "3", "1-2-3"]) {
      assert.throws(() => parseRange(text), /--range/);
    }
  });
});

describe("keyGreen", () => {
  it("чистый зелёный фон становится прозрачным", () => {
    const rgba = keyGreen(Buffer.from([10, 250, 5]));
    assert.equal(rgba[3], 0);
  });

  it("светлая кость остаётся непрозрачной и своего цвета", () => {
    assert.deepEqual([...keyGreen(Buffer.from([240, 225, 200]))], [240, 225, 200, 255]);
  });

  it("край получает промежуточную прозрачность, а зелёный прижимается к красному и синему", () => {
    const [red, green, blue, alpha] = keyGreen(Buffer.from([100, 170, 60]));
    assert.deepEqual([red, green, blue], [100, 100, 60]);
    assert.ok(alpha > 0 && alpha < 255, `прозрачность ${alpha}`);
  });
});

describe("findCycle", () => {
  it("находит длину повторяющегося движения", () => {
    const thumbs = Array.from({ length: 120 }, (_, i) => flat((i % 24) * 10));
    assert.equal(findCycle(thumbs).length, 24);
  });

  it("не принимает половину шага за целый, когда шаг левой и правой ногой лишь похожи", () => {
    // Вторая половина цикла повторяет первую с небольшим сдвигом яркости: как шаг другой ногой.
    const thumbs = Array.from({ length: 120 }, (_, i) => flat((i % 12) * 20 + (i % 24 < 12 ? 0 : 6)));
    assert.equal(findCycle(thumbs).length, 24);
  });

  it("сообщает, что повтора нет, если кадры не повторяются", () => {
    let seed = 20260928;
    const thumbs = Array.from({ length: 120 }, () => {
      seed = (seed * 48271) % 2147483647;
      return flat(seed % 256);
    });
    assert.throws(() => findCycle(thumbs), /--range/);
  });
});

describe("pickFrames", () => {
  it("берёт каждый второй кадр, когда нужно вдвое меньше", () => {
    assert.deepEqual(pickFrames({ start: 10, length: 8 }, 4), [10, 12, 14, 16]);
  });

  it("берёт все кадры отрезка, когда нужно столько же", () => {
    assert.deepEqual(pickFrames({ start: 0, length: 3 }, 3), [0, 1, 2]);
  });
});
