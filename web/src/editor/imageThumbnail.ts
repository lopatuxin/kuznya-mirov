import type { LoadedProjectImage } from "../projectLoader";
import type { Vec2 } from "./objectPlacement";

/** Сторона квадрата, в который вписан уменьшенный кадр, — «Редактор», «Окно редактора», требование 24. */
export const THUMBNAIL_SIZE_PX = 64;

/** Где кадр лежит в квадрате: вписан с сохранением пропорций, по середине. */
export function thumbnailRect(frame: Vec2): { x: number; y: number; width: number; height: number } {
  const scale = Math.min(THUMBNAIL_SIZE_PX / frame[0], THUMBNAIL_SIZE_PX / frame[1]);
  const width = frame[0] * scale;
  const height = frame[1] * scale;
  return { x: (THUMBNAIL_SIZE_PX - width) / 2, y: (THUMBNAIL_SIZE_PX - height) / 2, width, height };
}

/** Рисует первый кадр картинки в квадрат `THUMBNAIL_SIZE_PX`: без `smooth` точки остаются квадратиками, со `smooth` — смешиваются. */
export function drawThumbnail(canvas: HTMLCanvasElement, image: LoadedProjectImage, frame: Vec2, isSmooth: boolean): void {
  const target = canvas.getContext("2d");
  const source = document.createElement("canvas");
  source.width = image.width;
  source.height = image.height;
  const sourceContext = source.getContext("2d");
  if (target === null || sourceContext === null) return;
  const pixels = new Uint8ClampedArray(image.pixels.buffer as ArrayBuffer, image.pixels.byteOffset, image.pixels.byteLength);
  sourceContext.putImageData(new ImageData(pixels, image.width, image.height), 0, 0);
  const rect = thumbnailRect(frame);
  target.clearRect(0, 0, THUMBNAIL_SIZE_PX, THUMBNAIL_SIZE_PX);
  target.imageSmoothingEnabled = isSmooth;
  target.drawImage(source, 0, 0, frame[0], frame[1], rect.x, rect.y, rect.width, rect.height);
}
