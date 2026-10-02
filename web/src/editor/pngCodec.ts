import type { MaskBytes } from "./maskBytes";

const SIGNATURE = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
const COLOR_TYPE_GRAY = 0;
const COLOR_TYPE_RGB = 2;
const COLOR_TYPE_GRAY_ALPHA = 4;
const COLOR_TYPE_RGBA = 6;
const BIT_DEPTH = 8;
const CHANNELS_BY_COLOR_TYPE: Readonly<Record<number, number>> = {
  [COLOR_TYPE_GRAY]: 1,
  [COLOR_TYPE_RGB]: 3,
  [COLOR_TYPE_GRAY_ALPHA]: 2,
  [COLOR_TYPE_RGBA]: 4,
};
const IHDR_LENGTH = 13;
const CHUNK_OVERHEAD = 12;

const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (CRC_TABLE[(crc ^ byte) & 0xff] as number) ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

async function runStream(bytes: Uint8Array, transform: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(transform);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const result = new Uint8Array(CHUNK_OVERHEAD + data.length);
  const view = new DataView(result.buffer);
  view.setUint32(0, data.length);
  for (let index = 0; index < 4; index += 1) result[4 + index] = type.charCodeAt(index);
  result.set(data, 8);
  view.setUint32(8 + data.length, crc32(result.subarray(4, 8 + data.length)));
  return result;
}

/** PNG маски — «Покраска», требование 13: 8 бит, оттенки серого, без прозрачности и чересстрочности. */
export async function encodeMaskPng(mask: MaskBytes): Promise<Uint8Array> {
  const { width, height, pixels } = mask;
  const header = new Uint8Array(IHDR_LENGTH);
  const headerView = new DataView(header.buffer);
  headerView.setUint32(0, width);
  headerView.setUint32(4, height);
  header[8] = BIT_DEPTH;
  header[9] = COLOR_TYPE_GRAY;
  const rows = new Uint8Array(height * (width + 1));
  for (let row = 0; row < height; row += 1) rows.set(pixels.subarray(row * width, (row + 1) * width), row * (width + 1) + 1);
  const parts = [SIGNATURE, chunk("IHDR", header), chunk("IDAT", await runStream(rows, new CompressionStream("deflate"))), chunk("IEND", new Uint8Array(0))];
  const png = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    png.set(part, offset);
    offset += part.length;
  }
  return png;
}

function paeth(left: number, up: number, upLeft: number): number {
  const estimate = left + up - upLeft;
  const leftDistance = Math.abs(estimate - left);
  const upDistance = Math.abs(estimate - up);
  const upLeftDistance = Math.abs(estimate - upLeft);
  if (leftDistance <= upDistance && leftDistance <= upLeftDistance) return left;
  return upDistance <= upLeftDistance ? up : upLeft;
}

/** Снимает фильтры строк на месте; `stride` — байт на строку без байта фильтра, `pixelBytes` — байт на точку. */
function unfilterRows(data: Uint8Array, height: number, stride: number, pixelBytes: number): void {
  for (let row = 0; row < height; row += 1) {
    const rowStart = row * (stride + 1);
    const filter = data[rowStart] as number;
    for (let index = 0; index < stride; index += 1) {
      const position = rowStart + 1 + index;
      const left = index >= pixelBytes ? (data[position - pixelBytes] as number) : 0;
      const up = row > 0 ? (data[position - (stride + 1)] as number) : 0;
      const upLeft = row > 0 && index >= pixelBytes ? (data[position - (stride + 1) - pixelBytes] as number) : 0;
      let predictor: number;
      switch (filter) {
        case 0:
          predictor = 0;
          break;
        case 1:
          predictor = left;
          break;
        case 2:
          predictor = up;
          break;
        case 3:
          predictor = (left + up) >> 1;
          break;
        case 4:
          predictor = paeth(left, up, upLeft);
          break;
        default:
          throw new Error(`неизвестный фильтр строки ${filter}`);
      }
      data[position] = ((data[position] as number) + predictor) & 0xff;
    }
  }
}

type PngHeader = { width: number; height: number; colorType: number };

function readHeader(data: Uint8Array): PngHeader {
  if (data.length !== IHDR_LENGTH) throw new Error("заголовок IHDR не той длины");
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const header: PngHeader = { width: view.getUint32(0), height: view.getUint32(4), colorType: data[9] as number };
  if (data[8] !== BIT_DEPTH) throw new Error(`нужно 8 бит на канал, а в файле ${data[8]}`);
  if (CHANNELS_BY_COLOR_TYPE[header.colorType] === undefined) throw new Error(`вид цвета ${header.colorType} не поддерживается: нужны серый, серый с прозрачностью, RGB или RGBA`);
  if (data[10] !== 0 || data[11] !== 0) throw new Error("способ сжатия или фильтрации не тот, что у PNG");
  if (data[12] !== 0) throw new Error("чересстрочный PNG не поддерживается");
  if (header.width === 0 || header.height === 0) throw new Error("ширина и высота должны быть больше нуля");
  return header;
}

/**
 * Маска из PNG — «Покраска», требование 27: 8 бит, оттенки серого с прозрачностью и без, RGB и RGBA, все пять
 * фильтров строк, без чересстрочности; значение точки — первый канал. Не PNG или не такой PNG — ошибка с причиной.
 */
export async function decodeMaskPng(bytes: Uint8Array): Promise<MaskBytes> {
  if (bytes.length < SIGNATURE.length || SIGNATURE.some((value, index) => bytes[index] !== value)) throw new Error("это не PNG");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let header: PngHeader | null = null;
  const compressed: Uint8Array[] = [];
  for (let offset = SIGNATURE.length; offset + CHUNK_OVERHEAD <= bytes.length; ) {
    const length = view.getUint32(offset);
    const type = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (data.length !== length) throw new Error("файл оборван посреди блока");
    if (view.getUint32(offset + 8 + length) !== crc32(bytes.subarray(offset + 4, offset + 8 + length))) throw new Error(`блок ${type} повреждён: контрольная сумма не сходится`);
    if (type === "IHDR") header = readHeader(data);
    if (type === "IDAT") compressed.push(data);
    if (type === "IEND") break;
    offset += CHUNK_OVERHEAD + length;
  }
  if (header === null) throw new Error("нет блока IHDR");
  const joined = new Uint8Array(compressed.reduce((sum, part) => sum + part.length, 0));
  let joinedOffset = 0;
  for (const part of compressed) {
    joined.set(part, joinedOffset);
    joinedOffset += part.length;
  }
  let data: Uint8Array;
  try {
    data = await runStream(joined, new DecompressionStream("deflate"));
  } catch {
    throw new Error("данные картинки не разжимаются");
  }
  const pixelBytes = CHANNELS_BY_COLOR_TYPE[header.colorType] as number;
  const stride = header.width * pixelBytes;
  if (data.length !== header.height * (stride + 1)) throw new Error("данных картинки не столько, сколько нужно её размеру");
  unfilterRows(data, header.height, stride, pixelBytes);
  const pixels = new Uint8Array(header.width * header.height);
  for (let row = 0; row < header.height; row += 1) {
    for (let column = 0; column < header.width; column += 1) pixels[row * header.width + column] = data[row * (stride + 1) + 1 + column * pixelBytes] as number;
  }
  return { width: header.width, height: header.height, pixels };
}
