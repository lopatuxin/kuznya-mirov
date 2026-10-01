import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { grayPng } from "./png.mjs";
import { decodeGrayPng } from "./pngDecoder.mjs";

describe("grayPng", () => {
  it("файл разжимается в те же серые точки, размер и подпись верны", () => {
    const pixels = Uint8Array.from({ length: 7 * 5 }, (_, i) => (i * 37) % 256);
    const decoded = decodeGrayPng(grayPng(7, 5, pixels));
    assert.equal(decoded.width, 7);
    assert.equal(decoded.height, 5);
    assert.deepEqual(decoded.pixels, pixels);
  });

  it("точка 1 × 1 и весь отрезок значений 0–255", () => {
    assert.deepEqual(decodeGrayPng(grayPng(1, 1, Uint8Array.of(255))).pixels, Uint8Array.of(255));
    const ramp = Uint8Array.from({ length: 256 }, (_, i) => i);
    assert.deepEqual(decodeGrayPng(grayPng(256, 1, ramp)).pixels, ramp);
  });

  it("одни и те же точки — один и тот же файл", () => {
    const pixels = Uint8Array.from({ length: 64 * 64 }, (_, i) => (i >> 3) & 255);
    assert.deepEqual(grayPng(64, 64, pixels), grayPng(64, 64, pixels));
  });
});
