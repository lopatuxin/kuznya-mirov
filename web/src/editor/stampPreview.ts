/** Картинки штампа для окошка «Рельеф»: тень рельефа по его высотам — холмом и той же формой впадиной. */
export type StampPreview = { raised: string; lowered: string };

/** Размер картинки в точках — вдвое больше карточки, чтобы на плотных экранах она не мылилась. */
const PREVIEW_WIDTH = 256;
const PREVIEW_HEIGHT = 160;
/** Во сколько круче склоны на картинке: штамп целиком в карточке, и без этого он выглядит плоским. */
const RELIEF = 0.33;
/** Свет слева сверху, как на картах: склоны к нему светлые, от него — тёмные. */
const LIGHT: readonly [number, number, number] = normalized([-1, -1, 1.2]);

function normalized(vector: readonly [number, number, number]): [number, number, number] {
  const length = Math.hypot(vector[0], vector[1], vector[2]);
  return [vector[0] / length, vector[1] / length, vector[2] / length];
}

/** Высоты из текста файла штампа; не разбирается — `null`, картинки нет. */
export function readStampHeights(text: string | null): number[][] | null {
  if (text === null) return null;
  try {
    const rows = (JSON.parse(text) as { heights?: unknown } | null)?.heights;
    if (!Array.isArray(rows) || rows.length < 2 || !Array.isArray(rows[0]) || rows[0].length < 2) return null;
    return rows as number[][];
  } catch {
    return null;
  }
}

/** Высота штампа в месте `(u, v)` от 0 до 1 по ширине и высоте — между точками по прямой. */
function sampleStamp(heights: readonly (readonly number[])[], u: number, v: number): number {
  const rows = heights.length;
  const columns = (heights[0] as readonly number[]).length;
  const x = Math.min(Math.max(u, 0), 1) * (columns - 1);
  const y = Math.min(Math.max(v, 0), 1) * (rows - 1);
  const column = Math.min(Math.floor(x), columns - 2);
  const row = Math.min(Math.floor(y), rows - 2);
  const tx = x - column;
  const ty = y - row;
  const at = (dx: number, dy: number): number => (heights[row + dy] as readonly number[])[column + dx] as number;
  const top = at(0, 0) * (1 - tx) + at(1, 0) * tx;
  const bottom = at(0, 1) * (1 - tx) + at(1, 1) * tx;
  return top * (1 - ty) + bottom * ty;
}

/**
 * Точки картинки RGBA: штамп вписан в картинку с сохранением пропорций, поля по краям прозрачные. `sign` 1 — холм,
 * −1 — впадина: та же форма, вдавленная в землю. Цвет — земля, светлее к вершине холма и темнее ко дну впадины.
 */
export function shadeStamp(heights: readonly (readonly number[])[], width: number, height: number, sign: 1 | -1): Uint8ClampedArray<ArrayBuffer> {
  const pixels = new Uint8ClampedArray(width * height * 4);
  const aspect = (heights[0] as readonly number[]).length / heights.length;
  const fitWidth = Math.min(width, height * aspect);
  const fitHeight = fitWidth / aspect;
  const left = (width - fitWidth) / 2;
  const top = (height - fitHeight) / 2;
  const step = 1 / Math.max(fitWidth, fitHeight);
  for (let py = 0; py < height; py += 1) {
    for (let px = 0; px < width; px += 1) {
      const u = (px + 0.5 - left) / fitWidth;
      const v = (py + 0.5 - top) / fitHeight;
      if (u < 0 || u > 1 || v < 0 || v > 1) continue;
      const level = sign * sampleStamp(heights, u, v);
      const slopeX = (sign * (sampleStamp(heights, u + step, v) - sampleStamp(heights, u - step, v))) / (2 * step);
      const slopeY = (sign * (sampleStamp(heights, u, v + step) - sampleStamp(heights, u, v - step))) / (2 * step);
      const normal = normalized([-slopeX * RELIEF, -slopeY * RELIEF, 1]);
      const light = Math.max(0, normal[0] * LIGHT[0] + normal[1] * LIGHT[1] + normal[2] * LIGHT[2]);
      const shade = 0.45 + 0.75 * light;
      const base = sign > 0 ? [108 + level * 70, 128 + level * 55, 92 + level * 40] : [108 + level * 30, 128 + level * 20, 92 + level * 50];
      const offset = (py * width + px) * 4;
      pixels[offset] = (base[0] as number) * shade;
      pixels[offset + 1] = (base[1] as number) * shade;
      pixels[offset + 2] = (base[2] as number) * shade;
      pixels[offset + 3] = 255;
    }
  }
  return pixels;
}

/** Картинка адресом `data:`; браузер не дал рисовать на холсте — `null`. */
function pictureOf(heights: readonly (readonly number[])[], sign: 1 | -1): string | null {
  const canvas = document.createElement("canvas");
  canvas.width = PREVIEW_WIDTH;
  canvas.height = PREVIEW_HEIGHT;
  const context = canvas.getContext("2d");
  if (context === null) return null;
  context.putImageData(new ImageData(shadeStamp(heights, PREVIEW_WIDTH, PREVIEW_HEIGHT, sign), PREVIEW_WIDTH, PREVIEW_HEIGHT), 0, 0);
  return canvas.toDataURL();
}

/** Нарисованные картинки по тексту файла штампа: каждая перезагрузка проекта отдаёт те же тексты, рисовать их заново незачем. */
const drawnPreviews = new Map<string, StampPreview | null>();

/** Картинки штампа по тексту его файла; файл не разбирается — `null`. */
export function stampPreviewOf(text: string | null): StampPreview | null {
  if (text === null) return null;
  const drawn = drawnPreviews.get(text);
  if (drawn !== undefined) return drawn;
  const heights = readStampHeights(text);
  const raised = heights === null ? null : pictureOf(heights, 1);
  const lowered = heights === null ? null : pictureOf(heights, -1);
  const preview = raised === null || lowered === null ? null : { raised, lowered };
  drawnPreviews.set(text, preview);
  return preview;
}
