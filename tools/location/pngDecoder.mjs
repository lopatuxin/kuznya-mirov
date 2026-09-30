// Разбор серого PNG для тестов: проверяет подписи, суммы кусков и разжимает строки без фильтров.

import assert from "node:assert/strict";
import { crc32, inflateSync } from "node:zlib";

const SIGNATURE_LENGTH = 8;

/** Ширина, высота и серые точки PNG, который пишет `grayPng`: 8 бит, оттенки серого, без фильтров строк. */
export function decodeGrayPng(buffer) {
  assert.deepEqual([...buffer.subarray(1, 4)], [0x50, 0x4e, 0x47], "подпись PNG");
  const chunks = new Map();
  for (let at = SIGNATURE_LENGTH; at < buffer.length; ) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.toString("latin1", at + 4, at + 8);
    const data = buffer.subarray(at + 8, at + 8 + length);
    assert.equal(buffer.readUInt32BE(at + 8 + length), crc32(buffer.subarray(at + 4, at + 8 + length)), `сумма куска ${type}`);
    chunks.set(type, data);
    at += 12 + length;
  }
  const header = chunks.get("IHDR");
  const width = header.readUInt32BE(0);
  const height = header.readUInt32BE(4);
  assert.deepEqual([...header.subarray(8)], [8, 0, 0, 0, 0], "8 бит, оттенки серого, без чересстрочности");
  const rows = inflateSync(chunks.get("IDAT"));
  assert.equal(rows.length, (width + 1) * height);
  const pixels = new Uint8Array(width * height);
  for (let row = 0; row < height; row++) {
    assert.equal(rows[row * (width + 1)], 0, "фильтр строки — 0");
    pixels.set(rows.subarray(row * (width + 1) + 1, (row + 1) * (width + 1)), row * width);
  }
  return { width, height, pixels };
}
