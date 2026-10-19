// Растр поверх сцены: заливка многоугольников, линии-преграды и заливка от затравки. Им считаются вода
// для рельефа и цвет земли для карты цвета.

/** Растр `width × height` ячеек по `per` ячеек на клетку сцены; значения — числа в `Uint8Array`. */
export function createRaster(sceneWidth, sceneHeight, per) {
  const width = Math.ceil(sceneWidth * per);
  const height = Math.ceil(sceneHeight * per);
  return { width, height, per, data: new Uint8Array(width * height) };
}

/** Закрашивает значением `value` многоугольник `rings` (внешний контур и дыры, правило чётности) в клетках сцены. */
export function fillPolygon(raster, rings, value) {
  const { width, height, per, data } = raster;
  let minY = Infinity, maxY = -Infinity;
  for (const ring of rings) for (const [, y] of ring) { minY = Math.min(minY, y); maxY = Math.max(maxY, y); }
  const y0 = Math.max(0, Math.floor(minY * per));
  const y1 = Math.min(height - 1, Math.ceil(maxY * per));
  for (let row = y0; row <= y1; row++) {
    const sy = (row + 0.5) / per;
    const xs = [];
    for (const ring of rings) {
      for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
        const [xi, yi] = ring[i];
        const [xj, yj] = ring[j];
        if (yi > sy !== yj > sy) xs.push(((xj - xi) * (sy - yi)) / (yj - yi) + xi);
      }
    }
    xs.sort((a, b) => a - b);
    for (let k = 0; k + 1 < xs.length; k += 2) {
      const c0 = Math.max(0, Math.ceil(xs[k] * per - 0.5));
      const c1 = Math.min(width - 1, Math.floor(xs[k + 1] * per - 0.5));
      for (let col = c0; col <= c1; col++) data[row * width + col] = value;
    }
  }
}

/** Рисует линию `points` значением `value` без диагональных щелей: заливка от затравки её не перешагнёт. */
export function drawLine(raster, points, value) {
  const { width, height, per, data } = raster;
  const set = (c, r) => { if (c >= 0 && r >= 0 && c < width && r < height) data[r * width + c] = value; };
  for (let i = 0; i + 1 < points.length; i++) {
    let x = points[i][0] * per, y = points[i][1] * per;
    const x1 = points[i + 1][0] * per, y1 = points[i + 1][1] * per;
    const steps = Math.ceil(Math.max(Math.abs(x1 - x), Math.abs(y1 - y)) * 2) + 1;
    let pc = Math.floor(x), pr = Math.floor(y);
    set(pc, pr);
    for (let s = 1; s <= steps; s++) {
      const cx = x + ((x1 - x) * s) / steps;
      const cy = y + ((y1 - y) * s) / steps;
      const c = Math.floor(cx), r = Math.floor(cy);
      if (c !== pc && r !== pr) set(c, pr);
      set(c, r);
      pc = c; pr = r;
    }
  }
}

/** Заливает значением `value` всё связанное по сторонам с ячейкой затравки, где сейчас `from`. */
export function floodFill(raster, seed, from, value) {
  const { width, height, per, data } = raster;
  const c0 = Math.floor(seed[0] * per), r0 = Math.floor(seed[1] * per);
  if (c0 < 0 || r0 < 0 || c0 >= width || r0 >= height) return 0;
  if (data[r0 * width + c0] !== from) return 0;
  const stack = [r0 * width + c0];
  data[stack[0]] = value;
  let filled = 0;
  while (stack.length) {
    const i = stack.pop();
    filled++;
    const c = i % width, r = (i - c) / width;
    if (c > 0 && data[i - 1] === from) { data[i - 1] = value; stack.push(i - 1); }
    if (c < width - 1 && data[i + 1] === from) { data[i + 1] = value; stack.push(i + 1); }
    if (r > 0 && data[i - width] === from) { data[i - width] = value; stack.push(i - width); }
    if (r < height - 1 && data[i + width] === from) { data[i + width] = value; stack.push(i + width); }
  }
  return filled;
}

/** Значение растра в точке сцены; за краем — `outside`. */
export function sample(raster, x, y, outside = 0) {
  const c = Math.floor(x * raster.per), r = Math.floor(y * raster.per);
  if (c < 0 || r < 0 || c >= raster.width || r >= raster.height) return outside;
  return raster.data[r * raster.width + c];
}
