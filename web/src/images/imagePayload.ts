import { decodeImage, type DecodedImage } from "./imageLoader";
import { decodeVideo, type DecodedVideo } from "./videoLoader";

export type ImageEntry = { index: number; name: string; path: string };
/** Карта материала и маска покрытия читаются как картинки, но имени у них нет — «Свет и материалы», «Загрузка». */
export type ImageFileEntry = { index: number; path: string };
export type LoadedImage = { index: number; path: string; bytes: Uint8Array | null };
export type ImagePayloadEntry = { index: number } & DecodedImage;
export type VideoPayloadEntry = { index: number } & DecodedVideo;

/** Запись `files.images` с `path` на `.mp4` — видео, остальные — картинки («Картинки» → «Таблица картинок»). */
export function isVideoPath(path: string): boolean {
  return path.toLowerCase().endsWith(".mp4");
}

/**
 * По одному файлу на запись `read_texts().images` — все объявленные картинки, всегда: движок
 * сам решает при `load()`, ошибка ли ненайденный файл (объявлен ли он хоть где-то как `image`).
 * `readBinary` приходит от вызывающей стороны (`ProjectFileReader`) — тот же сетевой контракт, что у
 * шрифтов и звука, здесь не дублируется.
 */
export async function fetchImageBytes(
  images: ImageFileEntry[],
  readBinary: (path: string) => Promise<Uint8Array | null>,
): Promise<LoadedImage[]> {
  const bytesList = await Promise.all(images.map((image) => readBinary(image.path)));
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

/** Приговор и проигрыватель на каждое видео — «Картинки» → «Видео»: проигрыватель заводит браузер, движок получает его по номеру. */
export async function buildVideoPayload(loaded: LoadedImage[]): Promise<VideoPayloadEntry[]> {
  const decoded = await Promise.all(loaded.map((video) => decodeVideo(video.bytes)));
  return loaded.map((video, i) => ({ index: video.index, ...decoded[i] }));
}
