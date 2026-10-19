// Геометрия карты города: перевод широты и долготы в клетки сцены, выпуклая оболочка, наименьший
// повёрнутый прямоугольник вокруг контура, площадь и попадание точки в многоугольник.
// Клетки сцены: x — вправо, y — вниз, как в `scene.json`; угол — по часовой стрелке, если смотреть сверху.

const METERS_PER_DEGREE = 111320;

/** Перевод в клетки для рамки `bbox` = [мин. широта, мин. долгота, макс. широта, макс. долгота] и клетки `cell` метров. */
export function projector(bbox, cell) {
  const [minLat, minLon, maxLat, maxLon] = bbox;
  const midLat = ((minLat + maxLat) / 2) * (Math.PI / 180);
  const perLat = METERS_PER_DEGREE / cell;
  const perLon = (METERS_PER_DEGREE * Math.cos(midLat)) / cell;
  return {
    width: Math.round((maxLon - minLon) * perLon),
    height: Math.round((maxLat - minLat) * perLat),
    cell,
    toCell: (lat, lon) => [(lon - minLon) * perLon, (maxLat - lat) * perLat],
  };
}

/** Площадь многоугольника `[[x, y], ...]` без знака. */
export function area(ring) {
  let sum = 0;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    sum += (ring[j][0] + ring[i][0]) * (ring[j][1] - ring[i][1]);
  }
  return Math.abs(sum) / 2;
}

/** Лежит ли точка внутри многоугольника (чётность пересечений). */
export function inside(point, ring) {
  const [x, y] = point;
  let hit = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i];
    const [xj, yj] = ring[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/** Выпуклая оболочка точек — монотонная цепь, против часовой стрелки в осях x вправо, y вверх. */
export function hull(points) {
  const pts = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (pts.length < 3) return pts;
  const cross = (o, a, b) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower = [];
  for (const p of pts) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 0) lower.pop();
    lower.push(p);
  }
  const upper = [];
  for (let i = pts.length - 1; i >= 0; i--) {
    const p = pts[i];
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 0) upper.pop();
    upper.push(p);
  }
  return lower.slice(0, -1).concat(upper.slice(0, -1));
}

/**
 * Наименьший по площади прямоугольник вокруг точек: `{ cx, cy, length, width, angle }`, где `length` ≥ `width`,
 * а `angle` — направление длинной стороны в градусах от оси x по часовой стрелке (y вниз), от 0 до 180.
 */
export function orientedBox(points) {
  const h = hull(points);
  if (h.length === 0) throw new Error("пустой контур");
  if (h.length < 3) {
    const [a, b = a] = h;
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return { cx: (a[0] + b[0]) / 2, cy: (a[1] + b[1]) / 2, length, width: 0, angle: normAngle(Math.atan2(b[1] - a[1], b[0] - a[0])) };
  }
  let best = null;
  for (let i = 0; i < h.length; i++) {
    const a = h[i];
    const b = h[(i + 1) % h.length];
    const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
    if (len === 0) continue;
    const ux = (b[0] - a[0]) / len;
    const uy = (b[1] - a[1]) / len;
    let minU = Infinity, maxU = -Infinity, minV = Infinity, maxV = -Infinity;
    for (const p of h) {
      const u = p[0] * ux + p[1] * uy;
      const v = -p[0] * uy + p[1] * ux;
      minU = Math.min(minU, u); maxU = Math.max(maxU, u);
      minV = Math.min(minV, v); maxV = Math.max(maxV, v);
    }
    const sizeU = maxU - minU;
    const sizeV = maxV - minV;
    const boxArea = sizeU * sizeV;
    if (!best || boxArea < best.area - 1e-12) {
      const mu = (minU + maxU) / 2;
      const mv = (minV + maxV) / 2;
      best = { area: boxArea, cx: mu * ux - mv * uy, cy: mu * uy + mv * ux, sizeU, sizeV, ux, uy };
    }
  }
  const alongU = best.sizeU >= best.sizeV;
  const dir = alongU ? Math.atan2(best.uy, best.ux) : Math.atan2(best.ux, -best.uy);
  return {
    cx: best.cx,
    cy: best.cy,
    length: alongU ? best.sizeU : best.sizeV,
    width: alongU ? best.sizeV : best.sizeU,
    angle: normAngle(dir),
  };
}

function normAngle(radians) {
  let deg = (radians * 180) / Math.PI;
  deg = ((deg % 180) + 180) % 180;
  return Math.abs(deg - 180) < 1e-9 ? 0 : deg;
}

/** Запись объекта-прямоугольника: середина `cx, cy`, размер вдоль угла `length × width`, угол в градусах. */
export function rectRecord(cx, cy, length, width, angle) {
  return {
    position: [round(cx - length / 2), round(cy - width / 2)],
    size: [round(length), round(width)],
    rotation: round(angle, 2),
  };
}

/** Ближайшая к точке `p` точка отрезка `a`–`b` и расстояние до неё. */
export function nearestOnSegment(p, a, b) {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  const q = [a[0] + dx * t, a[1] + dy * t];
  return { point: q, distance: Math.hypot(p[0] - q[0], p[1] - q[1]) };
}

/** Отрезок `a`–`b`, обрезанный рамкой `[0, w] × [0, h]`, или `null`, если он целиком снаружи (Лианг — Барски). */
export function clipSegment(a, b, w, h) {
  let t0 = 0, t1 = 1;
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const checks = [[-dx, a[0]], [dx, w - a[0]], [-dy, a[1]], [dy, h - a[1]]];
  for (const [p, q] of checks) {
    if (p === 0) {
      if (q < 0) return null;
      continue;
    }
    const r = q / p;
    if (p < 0) { if (r > t1) return null; if (r > t0) t0 = r; }
    else { if (r < t0) return null; if (r < t1) t1 = r; }
  }
  return [[a[0] + dx * t0, a[1] + dy * t0], [a[0] + dx * t1, a[1] + dy * t1]];
}

/** Округление до `digits` знаков: файлы сцены не пухнут от длинных дробей. */
export function round(value, digits = 3) {
  const k = 10 ** digits;
  return Math.round(value * k) / k;
}

/** Детерминированный счётчик случайности (mulberry32): одно зерно — те же числа. */
export function random(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
