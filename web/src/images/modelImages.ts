import { decodeImage, type DecodedImage } from "./imageLoader";
import type { ImageFileEntry, LoadedImage } from "./imagePayload";

/** Модель из `read_texts().models`: номер, имя и путь файла `.glb` от папки игры; читается тем же `fetchImageBytes`, что картинки. */
export type ModelEntry = { name: string } & ImageFileEntry;
/** Сжатый файл картинки модели из `read_texts().model_textures`: номер и путь `.ktx2` от папки игры — «Трёхмерная сцена», «Загрузка и проверка». */
export type ModelTextureEntry = ImageFileEntry;
/** Ответ страницы по одному сжатому файлу для `load`: байты как есть, без разжатия; `null` — файла нет или он пуст. */
export type ModelTexturePayloadEntry = { index: number; bytes: Uint8Array | null };
/** Ответ страницы по одной модели для `load`: файл целиком и по каждой картинке из его `images` её номер в этом списке. */
export type ModelPayloadEntry = { index: number; bytes: Uint8Array | null; images: ({ index: number } & DecodedImage)[] };

const GLB_MAGIC = 0x46546c67;
const GLB_HEADER_BYTES = 12;
const CHUNK_HEADER_BYTES = 8;
const CHUNK_JSON = 0x4e4f534a;
const CHUNK_BIN = 0x004e4942;
const DECODABLE_MIME_TYPES: readonly string[] = ["image/png", "image/jpeg"];

type GltfImage = { uri?: unknown; bufferView?: unknown; mimeType?: unknown };
type GltfBufferView = { buffer?: unknown; byteOffset?: unknown; byteLength?: unknown };
type GltfJson = { images?: unknown; bufferViews?: unknown };

function findChunks(glb: Uint8Array): { json: Uint8Array; bin: Uint8Array | null } | null {
  if (glb.length < GLB_HEADER_BYTES) return null;
  const view = new DataView(glb.buffer, glb.byteOffset, glb.byteLength);
  if (view.getUint32(0, true) !== GLB_MAGIC) return null;
  let json: Uint8Array | null = null;
  let bin: Uint8Array | null = null;
  let offset = GLB_HEADER_BYTES;
  while (offset + CHUNK_HEADER_BYTES <= glb.length) {
    const length = view.getUint32(offset, true);
    const type = view.getUint32(offset + 4, true);
    const start = offset + CHUNK_HEADER_BYTES;
    if (start + length > glb.length) return null;
    const data = glb.subarray(start, start + length);
    if (type === CHUNK_JSON && json === null) json = data;
    if (type === CHUNK_BIN && bin === null) bin = data;
    offset = start + length;
  }
  return json === null ? null : { json, bin };
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

/** Байты одной картинки из буфера 0 файла; `null` — внешний `uri`, пустой вид, другой тип, чем PNG и JPEG, или вид за пределами буфера. */
function imageBytesOf(image: GltfImage, bufferViews: GltfBufferView[], bin: Uint8Array | null): Uint8Array | null {
  if (image.uri !== undefined || bin === null || typeof image.mimeType !== "string" || !DECODABLE_MIME_TYPES.includes(image.mimeType)) return null;
  if (!isNonNegativeInteger(image.bufferView)) return null;
  const bufferView = bufferViews[image.bufferView];
  if (bufferView === undefined || (bufferView.buffer ?? 0) !== 0) return null;
  const byteOffset = bufferView.byteOffset ?? 0;
  const byteLength = bufferView.byteLength;
  if (!isNonNegativeInteger(byteOffset) || !isNonNegativeInteger(byteLength) || byteLength === 0 || byteOffset + byteLength > bin.length) return null;
  return bin.subarray(byteOffset, byteOffset + byteLength);
}

/**
 * Картинки внутри `.glb` — «Трёхмерная сцена» → «Модели и движения»: по номеру в `images` файла байты PNG или JPEG из
 * буфера 0 по `bufferView`, `null` для остальных. Файл, что не разбирается, картинок не даёт — ошибку про сам файл назовёт движок.
 */
export function readModelImageBytes(glb: Uint8Array): (Uint8Array | null)[] {
  const chunks = findChunks(glb);
  if (chunks === null) return [];
  let gltf: GltfJson;
  try {
    gltf = JSON.parse(new TextDecoder().decode(chunks.json)) as GltfJson;
  } catch {
    return [];
  }
  if (!Array.isArray(gltf.images)) return [];
  const bufferViews = Array.isArray(gltf.bufferViews) ? (gltf.bufferViews as GltfBufferView[]) : [];
  return (gltf.images as GltfImage[]).map((image) => imageBytesOf(image, bufferViews, chunks.bin));
}

async function decodeModelImages(glb: Uint8Array): Promise<ModelPayloadEntry["images"]> {
  const imageBytes = readModelImageBytes(glb);
  const decoded = await Promise.all(imageBytes.map((bytes): Promise<DecodedImage> => (bytes === null ? Promise.resolve({ verdict: "rejected" }) : decodeImage(bytes))));
  return decoded.map((image, index) => ({ index, ...image }));
}

/** Файл и разжатые картинки каждой модели — ответ `load` по `models`; движок байтов картинок внутри файла не разжимает. */
export async function buildModelPayload(loaded: LoadedImage[]): Promise<ModelPayloadEntry[]> {
  return Promise.all(
    loaded.map(async (model) => ({
      index: model.index,
      bytes: model.bytes,
      images: model.bytes === null ? [] : await decodeModelImages(model.bytes),
    })),
  );
}

/** Сжатые файлы картинок моделей уходят в `load` как есть: BC и уменьшенные копии лежат в файле, разжимать браузеру нечего. */
export function buildModelTexturePayload(loaded: LoadedImage[]): ModelTexturePayloadEntry[] {
  return loaded.map((texture) => ({ index: texture.index, bytes: texture.bytes === null || texture.bytes.length === 0 ? null : texture.bytes }));
}
