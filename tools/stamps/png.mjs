// Разбор PNG плиток высот: 8 бит, RGB и RGBA, без чересстрочности, все пять фильтров строк. Разборщик
// свой, на `node:zlib`, чтобы у программы вырезки не было зависимостей.

import { crc32, inflateSync } from "node:zlib";

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const CHANNELS = new Map([
  [2, 3],
  [6, 4],
]);

function paeth(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const toLeft = Math.abs(estimate - left);
  const toUp = Math.abs(estimate - up);
  const toUpLeft = Math.abs(estimate - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) return left;
  return toUp <= toUpLeft ? up : upLeft;
}

/** Снимает фильтр строки `type` на месте: `row` — её байты, `previous` — готовая строка выше, `step` — байтов на точку. */
function unfilter(type, row, previous, step) {
  for (let i = 0; i < row.length; i++) {
    const left = i >= step ? row[i - step] : 0;
    const up = previous[i];
    const upLeft = i >= step ? previous[i - step] : 0;
    switch (type) {
      case 0:
        break;
      case 1:
        row[i] += left;
        break;
      case 2:
        row[i] += up;
        break;
      case 3:
        row[i] += (left + up) >> 1;
        break;
      case 4:
        row[i] += paeth(left, up, upLeft);
        break;
      default:
        throw new Error(`неизвестный фильтр строки ${type}`);
    }
  }
}

/** Ширина, высота, число байт на точку и точки PNG (`Uint8Array`, строки сверху вниз). */
export function decodePng(buffer) {
  if (buffer.length < SIGNATURE.length || !buffer.subarray(0, SIGNATURE.length).equals(SIGNATURE)) throw new Error("это не PNG: нет подписи");
  let header;
  const data = [];
  for (let at = SIGNATURE.length; at + 12 <= buffer.length; ) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.toString("latin1", at + 4, at + 8);
    const body = buffer.subarray(at + 8, at + 8 + length);
    if (at + 12 + length > buffer.length || buffer.readUInt32BE(at + 8 + length) !== crc32(buffer.subarray(at + 4, at + 8 + length))) {
      throw new Error(`PNG повреждён: сумма куска ${type} не сошлась`);
    }
    if (type === "IHDR") header = body;
    if (type === "IDAT") data.push(body);
    if (type === "IEND") break;
    at += 12 + length;
  }
  if (!header || data.length === 0) throw new Error("PNG повреждён: нет заголовка или точек");
  const width = header.readUInt32BE(0);
  const height = header.readUInt32BE(4);
  const [depth, colorType, , , interlace] = header.subarray(8, 13);
  const channels = CHANNELS.get(colorType);
  if (depth !== 8 || channels === undefined || interlace !== 0) {
    throw new Error(`PNG не поддержан: нужны 8 бит, RGB или RGBA, без чересстрочности (тут ${depth} бит, цвет ${colorType}, чересстрочность ${interlace})`);
  }
  const stride = width * channels;
  const rows = inflateSync(Buffer.concat(data));
  if (rows.length !== (stride + 1) * height) throw new Error("PNG повреждён: точек меньше, чем нужно по размеру");
  const pixels = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const row = pixels.subarray(y * stride, (y + 1) * stride);
    row.set(rows.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    unfilter(rows[y * (stride + 1)], row, y === 0 ? new Uint8Array(stride) : pixels.subarray((y - 1) * stride, y * stride), channels);
  }
  return { width, height, channels, pixels };
}
