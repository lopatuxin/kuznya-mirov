// PNG для тестов: кодировщик, который умеет все пять фильтров строк и подмену заголовка.

import { crc32, deflateSync } from "node:zlib";

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const check = Buffer.alloc(4);
  check.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, check]);
}

function paeth(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const toLeft = Math.abs(estimate - left);
  const toUp = Math.abs(estimate - up);
  const toUpLeft = Math.abs(estimate - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) return left;
  return toUp <= toUpLeft ? up : upLeft;
}

/**
 * PNG из `pixels` — `width × height × channels` байт по строкам сверху вниз. `filters` — фильтр каждой
 * строки по кругу (по умолчанию без фильтра); `header` подменяет поля заголовка: `depth`, `colorType`, `interlace`.
 */
export function encodePng({ width, height, channels, pixels, filters = [0], header = {} }) {
  const stride = width * channels;
  const rows = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const filter = filters[y % filters.length];
    rows[y * (stride + 1)] = filter;
    for (let i = 0; i < stride; i++) {
      const raw = pixels[y * stride + i];
      const left = i >= channels ? pixels[y * stride + i - channels] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + i] : 0;
      const upLeft = y > 0 && i >= channels ? pixels[(y - 1) * stride + i - channels] : 0;
      const predicted = [0, left, up, (left + up) >> 1, paeth(left, up, upLeft)][filter];
      rows[y * (stride + 1) + 1 + i] = (raw - predicted) & 255;
    }
  }
  const head = Buffer.alloc(13);
  head.writeUInt32BE(width, 0);
  head.writeUInt32BE(height, 4);
  head[8] = header.depth ?? 8;
  head[9] = header.colorType ?? (channels === 4 ? 6 : 2);
  head[12] = header.interlace ?? 0;
  return Buffer.concat([SIGNATURE, chunk("IHDR", head), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}
