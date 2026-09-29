// Резка нарисованного куска на полоски шириной в клетку («Фаза-02-5-первая-нарисованная-локация»,
// требование 13): низ каждой полоски — нижняя непрозрачная точка её же столбца картинки, пустота
// сверху добита прозрачным, так что все полоски получаются одной высоты (все кадры набора должны
// быть одного размера — «Картинки»). Кусок к этому моменту уже обработан `tools/art/` (фон снят,
// обрезан по содержимому, приведён к масштабу), эти функции только режут готовые точки на кадры.
// Чистые функции — тот же порог видимости точки, что у `tools/art/sheet.mjs` (`VISIBLE_ALPHA`).

const VISIBLE_ALPHA = 26;

function columnBottomRow(rgba, width, height, startX, columnWidth) {
  const endX = Math.min(width, startX + columnWidth);
  for (let y = height - 1; y >= 0; y--) {
    for (let x = startX; x < endX; x++) {
      if (rgba[(y * width + x) * 4 + 3] >= VISIBLE_ALPHA) return y;
    }
  }
  return -1; // столбец целиком прозрачен (крайняя полоска у`же самого столбца плюс запас)
}

// Нижняя непрозрачная строка каждого столбца шириной `columnWidth` слева направо; `-1` — столбец
// целиком прозрачен (полоску для него строить не из чего, вызывающий код её пропускает).
export function columnBottoms(rgba, width, height, columnWidth) {
  const columns = Math.ceil(width / columnWidth);
  const bottoms = [];
  for (let i = 0; i < columns; i++) bottoms.push(columnBottomRow(rgba, width, height, i * columnWidth, columnWidth));
  return bottoms;
}

// Лист кадров: каждый кадр — свой столбец, сдвинутый по вертикали так, что его нижняя непрозрачная
// точка ложится на нижнюю строку кадра (требование 13); столбцы без непрозрачных точек остаются
// кадром из одних прозрачных точек. Все кадры — той же высоты `height`, что у исходной картинки.
export function buildBottomAlignedStrip(rgba, width, height, columnWidth, bottoms) {
  const frameCount = bottoms.length;
  const outWidth = frameCount * columnWidth;
  const out = Buffer.alloc(outWidth * height * 4);
  for (let i = 0; i < frameCount; i++) {
    const bottom = bottoms[i];
    if (bottom < 0) continue;
    const startX = i * columnWidth;
    const endX = Math.min(width, startX + columnWidth);
    const shift = height - 1 - bottom;
    for (let sy = 0; sy <= bottom; sy++) {
      const dy = sy + shift;
      for (let sx = startX; sx < endX; sx++) {
        const srcIdx = (sy * width + sx) * 4;
        const dstIdx = (dy * outWidth + (i * columnWidth + (sx - startX))) * 4;
        out[dstIdx] = rgba[srcIdx];
        out[dstIdx + 1] = rgba[srcIdx + 1];
        out[dstIdx + 2] = rgba[srcIdx + 2];
        out[dstIdx + 3] = rgba[srcIdx + 3];
      }
    }
  }
  return { data: out, width: outWidth, height, frameCount };
}

// Требование 14: препятствия куска — прямоугольники, лесенкой по диагонали (два вправо на одну
// вверх), слитые попарно, где соседняя по столбцу клетка стоит в той же строке. Клетки — в системе
// координат куска: столбец 0 — клетка (0, 0), дальше на восток и, каждые два столбца, на одну клетку
// к северу (y меньше).
export function diagonalStaircaseCells(columnCount) {
  const cells = [];
  for (let i = 0; i < columnCount; i++) {
    const rise = Math.floor(i / 2);
    cells.push({ x: i, y: rise === 0 ? 0 : -rise }); // -0 !== 0 в toEqual, а обе клетки первой пары в строке 0
  }
  return cells;
}

// Слияние произвольного набора клеток `{x, y}` (без повторов) в прямоугольники высотой в клетку:
// клетки одной строки, соседние по столбцу, сливаются в один прямоугольник. Общая функция — ею же
// выражена лесенка одной диагонали (`mergeDiagonalStaircase`), она же годится для отражённой стены
// (столбцы идут в мировых координатах в обратную сторону — сборщик сцены сам считает, какие клетки
// передать) и для лесенки невидимых препятствий под водой.
export function mergeCellsIntoRects(cells) {
  const columnsByRow = new Map();
  for (const { x, y } of cells) {
    if (!columnsByRow.has(y)) columnsByRow.set(y, []);
    columnsByRow.get(y).push(x);
  }
  const rects = [];
  for (const [y, xs] of columnsByRow) {
    xs.sort((a, b) => a - b);
    let i = 0;
    while (i < xs.length) {
      let j = i;
      while (j + 1 < xs.length && xs[j + 1] === xs[j] + 1) j++;
      rects.push({ x: xs[i], y, width: xs[j] - xs[i] + 1, height: 1 });
      i = j + 1;
    }
  }
  return rects;
}

export function mergeDiagonalStaircase(columnCount) {
  return mergeCellsIntoRects(diagonalStaircaseCells(columnCount));
}
