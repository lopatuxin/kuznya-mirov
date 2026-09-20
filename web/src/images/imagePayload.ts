import { decodeImage, type DecodedImage } from "./imageLoader";

export type ImageEntry = { index: number; name: string; path: string };
export type LoadedImage = { index: number; path: string; bytes: Uint8Array | null };
export type ImagePayloadEntry = { index: number } & DecodedImage;

/**
 * По одному файлу на запись `read_texts().images` — все объявленные картинки, всегда: движок
 * сам решает при `load()`, ошибка ли ненайденный файл (объявлен ли он хоть где-то как `image`).
 * `fetchBinary` приходит от вызывающей стороны (`main.ts`) — тот же сетевой контракт, что у
 * шрифтов и звука, здесь не дублируется.
 */
export async function fetchImageBytes(
  baseUrl: string,
  images: ImageEntry[],
  fetchBinary: (url: string) => Promise<Uint8Array | null>,
): Promise<LoadedImage[]> {
  const bytesList = await Promise.all(images.map((image) => fetchBinary(`${baseUrl}${image.path}`)));
  return images.map((image, index) => ({ index: image.index, path: image.path, bytes: bytesList[index] ?? null }));
}

/**
 * Приговор и точки на каждую картинку — «Картинки» → пункт 12: разжатие делает браузер
 * (`decodeImage`), движок только получает готовый результат по номеру.
 */
export async function buildImagePayload(loaded: LoadedImage[]): Promise<ImagePayloadEntry[]> {
  const decoded = await Promise.all(loaded.map((image) => decodeImage(image.bytes)));
  return loaded.map((image, i) => ({ index: image.index, ...decoded[i] }));
}
