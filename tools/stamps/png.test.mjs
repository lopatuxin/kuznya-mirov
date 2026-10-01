import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { decodePng } from "./png.mjs";
import { encodePng } from "./testPng.mjs";

function picture(width, height, channels) {
  return Uint8Array.from({ length: width * height * channels }, (_, i) => (i * 37 + (i >> 3) * 11 + ((i * i) >> 2)) & 255);
}

describe("decodePng", () => {
  for (const channels of [3, 4]) {
    const name = channels === 3 ? "RGB" : "RGBA";
    for (const [filter, label] of ["без фильтра", "Sub", "Up", "Average", "Paeth"].entries()) {
      it(`${name}, фильтр строк ${label}`, () => {
        const pixels = picture(9, 6, channels);
        const decoded = decodePng(encodePng({ width: 9, height: 6, channels, pixels, filters: [filter] }));
        assert.deepEqual([decoded.width, decoded.height, decoded.channels], [9, 6, channels]);
        assert.deepEqual(decoded.pixels, pixels);
      });
    }

    it(`${name}, все пять фильтров в одной картинке по кругу`, () => {
      const pixels = picture(13, 11, channels);
      assert.deepEqual(decodePng(encodePng({ width: 13, height: 11, channels, pixels, filters: [4, 3, 2, 1, 0] })).pixels, pixels);
    });
  }

  it("точка 1 × 1", () => {
    assert.deepEqual(decodePng(encodePng({ width: 1, height: 1, channels: 3, pixels: Uint8Array.of(1, 2, 3), filters: [4] })).pixels, Uint8Array.of(1, 2, 3));
  });

  it("не PNG, испорченная сумма куска, обрезанный файл — ошибка", () => {
    const good = encodePng({ width: 2, height: 2, channels: 3, pixels: picture(2, 2, 3) });
    assert.throws(() => decodePng(Buffer.from("<html>")), /нет подписи/);
    const broken = Buffer.from(good);
    broken[broken.length - 20] ^= 0xff;
    assert.throws(() => decodePng(broken), /сумма куска/);
    assert.throws(() => decodePng(good.subarray(0, good.length - 20)), /PNG повреждён/);
  });

  it("серый, 16 бит и чересстрочный PNG не поддержаны", () => {
    const pixels = picture(2, 2, 3);
    for (const header of [{ colorType: 0 }, { depth: 16 }, { interlace: 1 }]) {
      assert.throws(() => decodePng(encodePng({ width: 2, height: 2, channels: 3, pixels, header })), /PNG не поддержан/);
    }
  });
});
