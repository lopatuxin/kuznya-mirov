// Серый PNG — 8 бит на точку, без палитры и прозрачности: так пишутся маски покрытий. Кодировщик
// свой, на `node:zlib`, чтобы у построителя не было зависимостей.

import { crc32, deflateSync } from "node:zlib";

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const GRAYSCALE = 0;

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const check = Buffer.alloc(4);
  check.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, check]);
}

/** PNG из `pixels` — `width × height` байт по строкам сверху вниз. */
export function grayPng(width, height, pixels) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = GRAYSCALE;
  // Байты сжатия, фильтра и чересстрочности остаются нулевыми.
  const rows = Buffer.alloc((width + 1) * height);
  for (let row = 0; row < height; row++) {
    // Байт фильтра строки — 0, без фильтра.
    Buffer.from(pixels.buffer, pixels.byteOffset + row * width, width).copy(rows, row * (width + 1) + 1);
  }
  return Buffer.concat([SIGNATURE, chunk("IHDR", header), chunk("IDAT", deflateSync(rows, { level: 9 })), chunk("IEND", Buffer.alloc(0))]);
}

const RGBA = 6;

/** PNG из `pixels` — `width × height` точек по четыре байта (красный, зелёный, синий, прозрачность). */
export function rgbaPng(width, height, pixels) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = RGBA;
  const stride = width * 4;
  const rows = Buffer.alloc((stride + 1) * height);
  for (let row = 0; row < height; row++) {
    Buffer.from(pixels.buffer, pixels.byteOffset + row * stride, stride).copy(rows, row * (stride + 1) + 1);
  }
  return Buffer.concat([SIGNATURE, chunk("IHDR", header), chunk("IDAT", deflateSync(rows, { level: 9 })), chunk("IEND", Buffer.alloc(0))]);
}
