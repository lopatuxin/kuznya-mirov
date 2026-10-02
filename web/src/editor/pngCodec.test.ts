import { deflateSync, inflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { decodeMaskPng, encodeMaskPng } from "./pngCodec";

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = crc & 1 ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const result = new Uint8Array(12 + data.length);
  const view = new DataView(result.buffer);
  view.setUint32(0, data.length);
  result.set(Buffer.from(type, "latin1"), 4);
  result.set(data, 8);
  view.setUint32(8 + data.length, crc32(result.subarray(4, 8 + data.length)));
  return result;
}

function paethPredictor(left: number, up: number, upLeft: number): number {
  const estimate = left + up - upLeft;
  const distances = [Math.abs(estimate - left), Math.abs(estimate - up), Math.abs(estimate - upLeft)];
  const best = Math.min(...distances);
  return [left, up, upLeft][distances.indexOf(best)] as number;
}

/** Строка в фильтре `filter` — прямое преобразование, независимое от разборщика. */
function filterRow(filter: number, row: Uint8Array, previous: Uint8Array, pixelBytes: number): Uint8Array {
  const result = new Uint8Array(row.length + 1);
  result[0] = filter;
  row.forEach((value, index) => {
    const left = index >= pixelBytes ? (row[index - pixelBytes] as number) : 0;
    const up = previous[index] as number;
    const upLeft = index >= pixelBytes ? (previous[index - pixelBytes] as number) : 0;
    const predictor = [0, left, up, (left + up) >> 1, paethPredictor(left, up, upLeft)][filter] as number;
    result[index + 1] = (value - predictor) & 0xff;
  });
  return result;
}

type PngOptions = { width: number; height: number; colorType: number; bitDepth?: number; interlace?: number; filter?: number };

/** PNG из сырых байт точек (`pixelBytes` на точку), собранный `node:zlib` и своим прямым фильтром. */
function buildPng(raw: Uint8Array, { width, height, colorType, bitDepth = 8, interlace = 0, filter = 0 }: PngOptions): Uint8Array {
  const pixelBytes = { 0: 1, 2: 3, 4: 2, 6: 4 }[colorType] ?? 1;
  const stride = width * pixelBytes;
  const rows: Uint8Array[] = [];
  for (let row = 0; row < height; row += 1) {
    const previous = row === 0 ? new Uint8Array(stride) : raw.subarray((row - 1) * stride, row * stride);
    rows.push(filterRow(filter, raw.subarray(row * stride, (row + 1) * stride), previous, pixelBytes));
  }
  const header = new Uint8Array(13);
  const view = new DataView(header.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  header.set([bitDepth, colorType, 0, 0, interlace], 8);
  return Buffer.concat([Buffer.from(SIGNATURE), chunk("IHDR", header), chunk("IDAT", deflateSync(Buffer.concat(rows))), chunk("IEND", new Uint8Array(0))]);
}

/** Точки 5 × 4 с крутыми перепадами: каждый фильтр строк здесь даёт не тот же набор байт, что отсутствие фильтра. */
function sampleRaw(pixelBytes: number): { raw: Uint8Array; firstChannel: number[] } {
  const raw = new Uint8Array(5 * 4 * pixelBytes);
  raw.forEach((_, index) => {
    raw[index] = (index * 53 + (index % 7) * 31) & 0xff;
  });
  return { raw, firstChannel: Array.from({ length: 20 }, (_, point) => raw[point * pixelBytes] as number) };
}

describe("decodeMaskPng", () => {
  const colorTypes = [
    { name: "серый", colorType: 0, pixelBytes: 1 },
    { name: "серый с прозрачностью", colorType: 4, pixelBytes: 2 },
    { name: "RGB", colorType: 2, pixelBytes: 3 },
    { name: "RGBA", colorType: 6, pixelBytes: 4 },
  ];

  for (const { name, colorType, pixelBytes } of colorTypes) {
    for (const filter of [0, 1, 2, 3, 4]) {
      it(`${name}, фильтр строк ${filter}: значение точки — первый канал`, async () => {
        const { raw, firstChannel } = sampleRaw(pixelBytes);

        const mask = await decodeMaskPng(buildPng(raw, { width: 5, height: 4, colorType, filter }));

        expect([mask.width, mask.height]).toEqual([5, 4]);
        expect(Array.from(mask.pixels)).toEqual(firstChannel);
      });
    }
  }

  it("не PNG — ошибка с причиной", async () => {
    await expect(decodeMaskPng(Uint8Array.of(1, 2, 3))).rejects.toThrow("это не PNG");
  });

  it("16 бит на канал не поддерживается", async () => {
    const png = buildPng(new Uint8Array(2 * 2 * 2), { width: 2, height: 2, colorType: 0, bitDepth: 16 });

    await expect(decodeMaskPng(png)).rejects.toThrow("нужно 8 бит");
  });

  it("палитра не поддерживается", async () => {
    const png = buildPng(new Uint8Array(4), { width: 2, height: 2, colorType: 3 });

    await expect(decodeMaskPng(png)).rejects.toThrow("вид цвета 3 не поддерживается");
  });

  it("чересстрочный PNG не поддерживается", async () => {
    const png = buildPng(new Uint8Array(4), { width: 2, height: 2, colorType: 0, interlace: 1 });

    await expect(decodeMaskPng(png)).rejects.toThrow("чересстрочный");
  });

  it("испорченный байт — контрольная сумма блока не сходится", async () => {
    const png = buildPng(new Uint8Array(4), { width: 2, height: 2, colorType: 0 });
    png[20] = (png[20] as number) ^ 0xff;

    await expect(decodeMaskPng(png)).rejects.toThrow("контрольная сумма");
  });

  it("оборванный файл — ошибка, а не падение", async () => {
    const png = buildPng(new Uint8Array(4), { width: 2, height: 2, colorType: 0 });

    await expect(decodeMaskPng(png.subarray(0, 40))).rejects.toThrow();
  });
});

describe("encodeMaskPng", () => {
  const mask = { width: 7, height: 5, pixels: Uint8Array.from({ length: 35 }, (_, index) => (index * 37) & 0xff) };

  it("пишет PNG: 8 бит, оттенки серого, без прозрачности и чересстрочности", async () => {
    const png = await encodeMaskPng(mask);
    const header = png.subarray(16, 29);

    expect(Array.from(png.subarray(0, 8))).toEqual(SIGNATURE);
    expect(Array.from(header.subarray(8))).toEqual([8, 0, 0, 0, 0]);
    expect(new DataView(png.buffer, png.byteOffset).getUint32(16)).toBe(7);
    expect(new DataView(png.buffer, png.byteOffset).getUint32(20)).toBe(5);
  });

  it("данные сжаты по zlib: независимый разборщик получает те же строки", async () => {
    const png = await encodeMaskPng(mask);
    const length = new DataView(png.buffer, png.byteOffset).getUint32(33);
    const rows = inflateSync(png.subarray(41, 41 + length));

    expect(rows.length).toBe(5 * (7 + 1));
    expect(Array.from(rows.subarray(1, 8))).toEqual(Array.from(mask.pixels.subarray(0, 7)));
  });

  it("разборщик возвращает те же точки байт в байт", async () => {
    const decoded = await decodeMaskPng(await encodeMaskPng(mask));

    expect(decoded.width).toBe(7);
    expect(decoded.height).toBe(5);
    expect(Array.from(decoded.pixels)).toEqual(Array.from(mask.pixels));
  });

  it("маска побольше — все 256 значений и неквадратный размер возвращаются без потерь", async () => {
    const big = { width: 64, height: 24, pixels: Uint8Array.from({ length: 64 * 24 }, (_, index) => index % 256) };

    const decoded = await decodeMaskPng(await encodeMaskPng(big));

    expect(Array.from(decoded.pixels)).toEqual(Array.from(big.pixels));
  });
});
