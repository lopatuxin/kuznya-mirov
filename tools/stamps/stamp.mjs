// Вырез штампа из плиток высот: прямоугольник на земле, сжатый до сетки точек, с вычтенным основанием
// и плавно погашенным краем. Штамп — высоты от 0 до 1, строки сверху вниз, первая строка — север.

import { TILE_SIZE } from "./tiles.mjs";

const EARTH_RADIUS = 6378137; // метров, сфера Web Mercator
const EDGE_POWER = 2.5; // степень, по которой края выреза закруглены между кругом и прямоугольником
const THOUSANDTHS = 1000;

function smoothstep(t) {
  const clamped = Math.min(1, Math.max(0, t));
  return clamped * clamped * (3 - 2 * clamped);
}

/** Точка мира в пикселях плиток, если мир — `worldPixels` пикселей по краю: x растёт на восток, y — на юг. */
function worldPoint(lat, lon, worldPixels) {
  return {
    x: ((lon + 180) / 360) * worldPixels,
    y: ((1 - Math.asinh(Math.tan((lat * Math.PI) / 180)) / Math.PI) / 2) * worldPixels,
  };
}

/** Индексы пикселей, чьи середины лежат в `[low, high)`, в пределах `min…max`; хотя бы один — даже у ячейки уже пикселя. */
function pixelSpan(low, high, min, max) {
  const first = Math.min(max, Math.max(min, Math.ceil(low - 0.5)));
  return [first, Math.min(max + 1, Math.max(first + 1, Math.ceil(high - 0.5)))];
}

/** Число точек выреза по ширине и глубине: длинная сторона — `points`, короткая — по пропорции. */
function gridSize({ km: [width, depth], points }) {
  if (width >= depth) return { cols: points, rows: Math.max(2, Math.round((points * depth) / width)) };
  return { cols: Math.max(2, Math.round((points * width) / depth)), rows: points };
}

/**
 * Режет штамп `cut` (поле списка с умолчаниями) из плиток, которые отдаёт `readTile(x, y, zoom)`. Возвращает
 * `{ heights }`: строки чисел от 0 до 1 с точностью до тысячных, наибольшее — 1.
 */
export async function cutStamp(cut, readTile) {
  const { lat, lon, km, base, fade, zoom, invert } = cut;
  const { cols, rows } = gridSize(cut);
  const worldPixels = TILE_SIZE * 2 ** zoom;
  const groundPerPixel = ((2 * Math.PI * EARTH_RADIUS) / worldPixels) * Math.cos((lat * Math.PI) / 180);
  const width = (km[0] * 1000) / groundPerPixel;
  const depth = (km[1] * 1000) / groundPerPixel;
  const center = worldPoint(lat, lon, worldPixels);
  const left = center.x - width / 2;
  const top = center.y - depth / 2;

  const tiles = new Map();
  for (let tileY = Math.floor(top / TILE_SIZE); tileY <= Math.floor((top + depth) / TILE_SIZE); tileY++) {
    for (let tileX = Math.floor(left / TILE_SIZE); tileX <= Math.floor((left + width) / TILE_SIZE); tileX++) {
      tiles.set(`${tileX}/${tileY}`, await readTile(tileX, tileY, zoom));
    }
  }
  const metersAt = (x, y) => {
    const tile = tiles.get(`${Math.floor(x / TILE_SIZE)}/${Math.floor(y / TILE_SIZE)}`);
    return tile[(y % TILE_SIZE) * TILE_SIZE + (x % TILE_SIZE)];
  };

  // Точка штампа стоит на узле сетки: первая и последняя точки — на краях выреза, ячейка вокруг узла — полшага в стороны.
  const stepX = width / (cols - 1);
  const stepY = depth / (rows - 1);
  const meters = new Float64Array(rows * cols);
  for (let row = 0; row < rows; row++) {
    const [y0, y1] = pixelSpan(top + Math.max(0, (row - 0.5) * stepY), top + Math.min(depth, (row + 0.5) * stepY), Math.floor(top), Math.floor(top + depth));
    for (let col = 0; col < cols; col++) {
      const [x0, x1] = pixelSpan(left + Math.max(0, (col - 0.5) * stepX), left + Math.min(width, (col + 0.5) * stepX), Math.floor(left), Math.floor(left + width));
      let sum = 0;
      for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) sum += metersAt(x, y);
      meters[row * cols + col] = sum / ((x1 - x0) * (y1 - y0));
    }
  }

  if (invert) {
    const highest = meters.reduce((most, value) => Math.max(most, value), -Infinity);
    for (let i = 0; i < meters.length; i++) meters[i] = highest - meters[i];
  }
  const floor = Float64Array.from(meters).sort()[Math.min(meters.length - 1, Math.floor(base * meters.length))];
  let peak = 0;
  for (let row = 0; row < rows; row++) {
    const v = Math.abs((2 * row) / (rows - 1) - 1);
    for (let col = 0; col < cols; col++) {
      const u = Math.abs((2 * col) / (cols - 1) - 1);
      const edge = (u ** EDGE_POWER + v ** EDGE_POWER) ** (1 / EDGE_POWER);
      const index = row * cols + col;
      meters[index] = Math.max(0, meters[index] - floor) * smoothstep((1 - edge) / fade);
      peak = Math.max(peak, meters[index]);
    }
  }
  if (peak === 0) throw new Error("после вычета основания высот не осталось — ровное место или слишком большое основание");
  return {
    heights: Array.from({ length: rows }, (_, row) =>
      Array.from(meters.subarray(row * cols, (row + 1) * cols), (value) => Math.round((value / peak) * THOUSANDTHS) / THOUSANDTHS),
    ),
  };
}

/** Файл штампа: `{ "heights": [` и по строке штампа на строку файла. */
export function stampText({ heights }) {
  const rows = heights.map((row, index) => `  [${row.join(", ")}]${index + 1 < heights.length ? "," : ""}`);
  return ['{ "heights": [', ...rows, "] }", ""].join("\n");
}
