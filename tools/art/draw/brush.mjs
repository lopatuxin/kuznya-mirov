// Общие кисти программ, которыми Claude рисует картинки первого уровня платформера:
// случайность с зерном, гладкие кривые через точки, переходы цвета, фактура и размытие.

/** Точек картинки на клетку сцены — `cell_pixels` платформера. */
export const CELL = 96;

/** Повторяемая случайность: одно зерно — одна и та же картинка. */
export function rng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  next.range = (lo, hi) => lo + (hi - lo) * next();
  next.int = (lo, hi) => Math.floor(lo + (hi - lo + 1) * next());
  next.pick = (list) => list[Math.floor(next() * list.length)];
  return next;
}

const f = (n) => (Math.round(n * 100) / 100).toString();

/** Замкнутая гладкая кривая через точки (Катмулл — Ром в кубические Безье). */
export function smoothClosed(points, tension = 1) {
  const n = points.length;
  let d = `M${f(points[0][0])},${f(points[0][1])}`;
  for (let i = 0; i < n; i++) {
    const p0 = points[(i - 1 + n) % n];
    const p1 = points[i];
    const p2 = points[(i + 1) % n];
    const p3 = points[(i + 2) % n];
    const c1 = [p1[0] + ((p2[0] - p0[0]) / 6) * tension, p1[1] + ((p2[1] - p0[1]) / 6) * tension];
    const c2 = [p2[0] - ((p3[0] - p1[0]) / 6) * tension, p2[1] - ((p3[1] - p1[1]) / 6) * tension];
    d += `C${f(c1[0])},${f(c1[1])} ${f(c2[0])},${f(c2[1])} ${f(p2[0])},${f(p2[1])}`;
  }
  return d + "Z";
}

/** Открытая гладкая кривая через точки; `move` — начинать ли новой линией. */
export function smoothOpen(points, tension = 1, move = true) {
  const n = points.length;
  let d = move ? `M${f(points[0][0])},${f(points[0][1])}` : `L${f(points[0][0])},${f(points[0][1])}`;
  for (let i = 0; i < n - 1; i++) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(n - 1, i + 2)];
    const c1 = [p1[0] + ((p2[0] - p0[0]) / 6) * tension, p1[1] + ((p2[1] - p0[1]) / 6) * tension];
    const c2 = [p2[0] - ((p3[0] - p1[0]) / 6) * tension, p2[1] - ((p3[1] - p1[1]) / 6) * tension];
    d += `C${f(c1[0])},${f(c1[1])} ${f(c2[0])},${f(c2[1])} ${f(p2[0])},${f(p2[1])}`;
  }
  return d;
}

/** Неровное пятно вокруг середины: `n` точек, отклонение радиуса — доля `jitter`. */
export function blob(rnd, cx, cy, rx, ry, n = 9, jitter = 0.18, turn = 0) {
  const pts = [];
  for (let i = 0; i < n; i++) {
    const a = turn + (i / n) * Math.PI * 2;
    const k = 1 + rnd.range(-jitter, jitter);
    pts.push([cx + Math.cos(a) * rx * k, cy + Math.sin(a) * ry * k]);
  }
  return smoothClosed(pts);
}

/** Цвет `#rrggbb` в тройку чисел и обратно. */
function hex(c) {
  const v = parseInt(c.slice(1), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}
function css([r, g, b]) {
  const h = (x) => Math.max(0, Math.min(255, Math.round(x))).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}
/** Смесь двух цветов: `t` 0 — первый, 1 — второй. */
export function mix(a, b, t) {
  const x = hex(a);
  const y = hex(b);
  return css([0, 1, 2].map((i) => x[i] + (y[i] - x[i]) * t));
}
/** Светлее к белому (`t` > 0) или темнее к чёрному (`t` < 0). */
export function shade(c, t) {
  return t >= 0 ? mix(c, "#ffffff", t) : mix(c, "#000000", -t);
}

/** Вертикальный или любой линейный переход цвета. */
export function linear(id, stops, x1 = 0, y1 = 0, x2 = 0, y2 = 1, units = "objectBoundingBox") {
  const s = stops
    .map(([o, c, a = 1]) => `<stop offset="${o}" stop-color="${c}" stop-opacity="${a}"/>`)
    .join("");
  return `<linearGradient id="${id}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" gradientUnits="${units}">${s}</linearGradient>`;
}
export function radial(id, stops, cx = 0.5, cy = 0.5, r = 0.5, units = "objectBoundingBox", fx = cx, fy = cy) {
  const s = stops
    .map(([o, c, a = 1]) => `<stop offset="${o}" stop-color="${c}" stop-opacity="${a}"/>`)
    .join("");
  return `<radialGradient id="${id}" cx="${cx}" cy="${cy}" r="${r}" fx="${fx}" fy="${fy}" gradientUnits="${units}">${s}</radialGradient>`;
}

/**
 * Фактура: шум, наложенный на нарисованное умножением — камень и мох не гладкие.
 * `freq` — мелкость зерна, `amount` — сила.
 */
export function grainFilter(id, freq = 0.9, amount = 0.18, seed = 1, octaves = 2) {
  const lo = 1 - amount;
  return `<filter id="${id}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">
    <feTurbulence type="fractalNoise" baseFrequency="${freq}" numOctaves="${octaves}" seed="${seed}" result="n"/>
    <feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0.33 0.33 0.33 0 0" result="g"/>
    <feComponentTransfer in="n" result="m"><feFuncR type="linear" slope="${2 * amount}" intercept="${lo}"/><feFuncG type="linear" slope="${2 * amount}" intercept="${lo}"/><feFuncB type="linear" slope="${2 * amount}" intercept="${lo}"/><feFuncA type="linear" slope="0" intercept="1"/></feComponentTransfer>
    <feBlend in="SourceGraphic" in2="m" mode="multiply" result="b"/>
    <feComposite in="b" in2="SourceGraphic" operator="in"/>
  </filter>`;
}

/** Размытие для мягких краёв дымки и теней. */
export function blurFilter(id, sd) {
  return `<filter id="${id}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="${sd}"/></filter>`;
}

export function svg(width, height, body, defs = "") {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><defs>${defs}</defs>${body}</svg>`;
}

/** Сужающаяся палка от точки `a` до `b` с толщиной `wa` и `wb` — рука, нога, ствол, травинка. */
export function capsule(a, b, wa, wb) {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len = Math.hypot(dx, dy) || 1e-6;
  const ux = dx / len;
  const uy = dy / len;
  const nx = -uy;
  const ny = ux;
  const ra = wa / 2;
  const rb = wb / 2;
  // Касательные к двум окружностям разного радиуса.
  const s = (ra - rb) / len;
  const c = Math.sqrt(Math.max(0, 1 - s * s));
  const ax1 = a[0] + ra * (nx * c + ux * s);
  const ay1 = a[1] + ra * (ny * c + uy * s);
  const bx1 = b[0] + rb * (nx * c + ux * s);
  const by1 = b[1] + rb * (ny * c + uy * s);
  const ax2 = a[0] + ra * (-nx * c + ux * s);
  const ay2 = a[1] + ra * (-ny * c + uy * s);
  const bx2 = b[0] + rb * (-nx * c + ux * s);
  const by2 = b[1] + rb * (-ny * c + uy * s);
  return `M${f(ax1)},${f(ay1)}L${f(bx1)},${f(by1)}A${f(rb)},${f(rb)} 0 0 0 ${f(bx2)},${f(by2)}L${f(ax2)},${f(ay2)}A${f(ra)},${f(ra)} 0 0 0 ${f(ax1)},${f(ay1)}Z`;
}

export { f as num };
