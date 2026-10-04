// Плитки высот Terrain Tiles (AWS Open Data) формата Terrarium: масштаб задаёт вырез, PNG 256 × 256, высота в
// метрах кодируется цветом точки. Скачанная плитка лежит в кэше и второй раз не качается.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { decodePng } from "./png.mjs";

export const TILE_SIZE = 256;
const DOWNLOAD_TIMEOUT_MS = 60000;
export const CACHE_DIR = fileURLToPath(new URL("./.cache/", import.meta.url));

export function tileUrl(x, y, zoom) {
  return `https://s3.amazonaws.com/elevation-tiles-prod/terrarium/${zoom}/${x}/${y}.png`;
}

/** Высота в метрах из цвета точки плитки Terrarium. */
export function terrariumHeight(red, green, blue) {
  return red * 256 + green + blue / 256 - 32768;
}

async function downloadTile(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS) });
  if (!response.ok) throw new Error(`ответ ${response.status} ${response.statusText}`);
  return Buffer.from(await response.arrayBuffer());
}

function tileHeights(buffer) {
  const { width, height, channels, pixels } = decodePng(buffer);
  if (width !== TILE_SIZE || height !== TILE_SIZE) throw new Error(`размер плитки ${width} × ${height}, нужен ${TILE_SIZE} × ${TILE_SIZE}`);
  const heights = new Float64Array(TILE_SIZE * TILE_SIZE);
  for (let i = 0; i < heights.length; i++) heights[i] = terrariumHeight(pixels[i * channels], pixels[i * channels + 1], pixels[i * channels + 2]);
  return heights;
}

/**
 * Читатель плиток: `(x, y, zoom)` → высоты плитки в метрах (`Float64Array`, строки сверху вниз). Плитка берётся
 * из `cacheDir`, а если её там нет — скачивается через `download(url)` и ложится в кэш. Плитка, что не
 * скачалась или не разобралась, — ошибка с её номером и адресом.
 */
export function createTileReader({ cacheDir = CACHE_DIR, download = downloadTile } = {}) {
  const loaded = new Map();
  const read = async (x, y, zoom) => {
    const file = join(cacheDir, `${zoom}-${x}-${y}.png`);
    const cached = await readFile(file).catch(() => null);
    const buffer = cached ?? (await download(tileUrl(x, y, zoom)));
    const heights = tileHeights(buffer);
    if (cached === null) {
      await mkdir(cacheDir, { recursive: true });
      await writeFile(file, buffer);
    }
    return heights;
  };
  return (x, y, zoom) => {
    const key = `${zoom}/${x}/${y}`;
    if (!loaded.has(key)) {
      loaded.set(
        key,
        read(x, y, zoom).catch((error) => {
          throw new Error(`плитка ${zoom}/${x}/${y} (${tileUrl(x, y, zoom)}): ${error.message}`);
        }),
      );
    }
    return loaded.get(key);
  };
}
