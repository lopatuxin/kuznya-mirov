import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { fadeEnds, findCycle, keyGreen, keyMagenta, parseRange, pickFrames, wrapEnds } from "./sheet.mjs";

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

describe("keyMagenta", () => {
  it("чистый розовый фон становится прозрачным", () => {
    const rgba = keyMagenta(Buffer.from([250, 10, 245]));
    assert.equal(rgba[3], 0);
  });

  it("трава и бурая земля остаются непрозрачными и своего цвета", () => {
    assert.deepEqual([...keyMagenta(Buffer.from([80, 140, 40]))], [80, 140, 40, 255]);
    assert.deepEqual([...keyMagenta(Buffer.from([200, 150, 90]))], [200, 150, 90, 255]);
  });

  it("край получает промежуточную прозрачность, а красный и синий снижаются на избыток над зелёным", () => {
    const [red, green, blue, alpha] = keyMagenta(Buffer.from([180, 100, 170]));
    assert.deepEqual([red, green, blue], [110, 100, 100]);
    assert.ok(alpha > 0 && alpha < 255, `прозрачность ${alpha}`);
  });
});

describe("fadeEnds", () => {
  it("гасит концы от крайних видимых точек, середину и прозрачные поля не трогает", () => {
    const alphas = [0, 255, 255, 255, 255, 255, 255, 255, 255, 0];
    const rgba = Buffer.from(alphas.flatMap((alpha) => [100, 80, 60, alpha]));
    fadeEnds(rgba, alphas.length, 1, 3);
    const faded = alphas.map((_, x) => rgba[x * 4 + 3]);
    assert.equal(faded[0], 0);
    assert.equal(faded[1], 0);
    assert.ok(faded[2] > 0 && faded[2] < 255, `прозрачность ${faded[2]}`);
    assert.equal(faded[4], 255);
    assert.equal(faded[5], 255);
    assert.equal(faded[8], 0);
    assert.equal(faded[9], 0);
  });
});

describe("wrapEnds", () => {
  it("укорачивает картинку на наложение и ставит в начало конец исходника, чтобы повтор шёл без шва", () => {
    const reds = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
    const rgba = Buffer.from(reds.flatMap((red) => [red, 0, 0, 255]));
    const out = wrapEnds(rgba, reds.length, 1, 3);
    const outReds = Array.from({ length: 7 }, (_, x) => out[x * 4]);
    assert.equal(outReds[0], 70);
    assert.deepEqual(outReds.slice(3), [30, 40, 50, 60]);
    assert.ok(outReds[1] < 80 && outReds[1] > 10, `смешанная точка ${outReds[1]}`);
  });

  it("прозрачная точка не темнит видимую, с которой смешивается", () => {
    // Вторая точка результата — поровну вторая точка исходника и прозрачная последняя.
    const pixels = [[0, 0, 0, 255], [200, 100, 50, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 0]];
    const out = wrapEnds(Buffer.from(pixels.flat()), pixels.length, 1, 2);
    assert.deepEqual([...out.subarray(4, 8)], [200, 100, 50, 128]);
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
