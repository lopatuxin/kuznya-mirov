export type ImageVerdict = "ok" | "missing" | "rejected";

/**
 * Ровно то, что «load» ждёт по одной картинке — «Картинки» → «Страница, чтение картинок»:
 * при `ok` размер и точки, при `missing`/`rejected` движок эти поля не читает вовсе.
 */
export type DecodedImage =
  | { verdict: "ok"; width: number; height: number; pixels: Uint8Array }
  | { verdict: "missing" | "rejected" };

/**
 * `bytes === null` — файл не нашёлся при третьем заходе загрузки (см. `fetchBinary` в
 * `main.ts`), это приговор `missing` без обращения к браузерному декодеру. Иначе разжатие
 * пробует `createImageBitmap`: файл, который браузер не смог разжать, — это `rejected`, и в
 * тексте предстартовой проверки видно, что приговор вынес браузер, а не движок.
 */
export async function decodeImage(bytes: Uint8Array | null): Promise<DecodedImage> {
  if (bytes === null) return { verdict: "missing" };

  let bitmap: ImageBitmap;
  try {
    // `premultiplyAlpha: "none"` и `colorSpaceConversion: "none"` — без домножения на
    // прозрачность и без пересчёта цветов под монитор (движок сам умножает `opacity` при
    // отрисовке). Точной копии файла это всё же не даёт: ниже картинка проходит через
    // 2D-канвас, который хранит точки домноженными и делит обратно в `getImageData`, поэтому
    // полупрозрачные точки теряют до единицы на канал, а у полностью прозрачных обнуляется
    // цвет. Способ предписан планом фазы 04: иначе разбор PNG пришлось бы держать в движке.
    bitmap = await createImageBitmap(new Blob([bytes as BlobPart]), {
      premultiplyAlpha: "none",
      colorSpaceConversion: "none",
    });
  } catch {
    return { verdict: "rejected" };
  }

  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  if (!context) {
    bitmap.close();
    return { verdict: "rejected" };
  }
  context.drawImage(bitmap, 0, 0);
  const { data } = context.getImageData(0, 0, bitmap.width, bitmap.height);
  const width = bitmap.width;
  const height = bitmap.height;
  bitmap.close();

  return { verdict: "ok", width, height, pixels: new Uint8Array(data.buffer, data.byteOffset, data.byteLength) };
}
