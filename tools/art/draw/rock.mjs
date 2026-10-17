// Расколотый камень: глыбы — ячейки Вороного, вытянутые по ширине, со скруглёнными углами.
import { smoothClosed, linear, shade } from "./brush.mjs";

function clip(poly, a, b, c) {
  // Оставляет часть многоугольника, где a·x + b·y <= c.
  const out = [];
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    const fp = a * p[0] + b * p[1] - c;
    const fq = a * q[0] + b * q[1] - c;
    if (fp <= 0) out.push(p);
    if ((fp < 0 && fq > 0) || (fp > 0 && fq < 0)) {
      const t = fp / (fp - fq);
      out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
    }
  }
  return out;
}

/** Ячейки Вороного в прямоугольнике; `sy` растягивает ячейки по ширине. */
function voronoi(points, box, sy = 2.1) {
  const [x0, y0, x1, y1] = box;
  return points.map((p, i) => {
    let poly = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    for (let j = 0; j < points.length && poly.length; j++) {
      if (j === i) continue;
      const q = points[j];
      // Серединный перпендикуляр в растянутых координатах (x, y·sy).
      const ax = q[0] - p[0];
      const by = (q[1] - p[1]) * sy * sy;
      const mx = (p[0] + q[0]) / 2;
      const my = (p[1] + q[1]) / 2;
      poly = clip(poly, ax, by, ax * mx + by * my);
    }
    return poly;
  });
}

/** Скруглённый многоугольник: углы срезаются и сглаживаются. */
function rounded(poly, inset, cut = 0.28) {
  const n = poly.length;
  const cx = poly.reduce((s, p) => s + p[0], 0) / n;
  const cy = poly.reduce((s, p) => s + p[1], 0) / n;
  const shrunk = poly.map(([x, y]) => {
    const dx = x - cx;
    const dy = y - cy;
    const d = Math.hypot(dx, dy) || 1;
    const k = Math.max(0, (d - inset) / d);
    return [cx + dx * k, cy + dy * k];
  });
  const pts = [];
  for (let i = 0; i < n; i++) {
    const p = shrunk[i];
    const q = shrunk[(i + 1) % n];
    pts.push([p[0] + (q[0] - p[0]) * cut, p[1] + (q[1] - p[1]) * cut]);
    pts.push([p[0] + (q[0] - p[0]) * (1 - cut), p[1] + (q[1] - p[1]) * (1 - cut)]);
  }
  return { d: smoothClosed(pts, 0.7), cx, cy };
}

/**
 * Камень во весь прямоугольник `box`: глыбы с переходом от света слева сверху к тени,
 * разные оттенки — серо-синий, тёплый, зеленоватый. Возвращает `defs` и `body`.
 */
export function rockFace(rnd, box, { size = 90, palette, idp = "r", grow = 0.5 } = {}) {
  const [x0, y0, x1, y1] = box;
  const pts = [];
  // Ниже — крупнее глыбы: шаг сетки растёт с глубиной.
  let y = y0 + size * 0.3;
  while (y < y1 + size) {
    const depth = (y - y0) / Math.max(1, y1 - y0);
    const step = size * (1 + depth * grow);
    let x = x0 - step * rnd.range(0, 1);
    while (x < x1 + step) {
      pts.push([x + rnd.range(-0.35, 0.35) * step, y + rnd.range(-0.3, 0.3) * step * 0.6]);
      x += step * rnd.range(0.8, 1.4);
    }
    y += step * 0.62;
  }
  const cells = voronoi(pts, [x0 - size, y0 - size, x1 + size, y1 + size]);
  const tones = palette ?? ["#8b9097", "#7e848c", "#958f86", "#848c86", "#9a9a94"];
  let defs = "";
  let body = "";
  cells.forEach((poly, i) => {
    if (poly.length < 3) return;
    const { d } = rounded(poly, rnd.range(1.2, 2.4), 0.22);
    const base = rnd.pick(tones);
    const t = rnd.range(-0.1, 0.08);
    const c = shade(base, t);
    const id = `${idp}${i}`;
    const ang = rnd.range(-0.1, 0.1);
    defs += linear(id, [[0, shade(c, 0.36)], [0.18, shade(c, 0.16)], [0.55, c], [0.85, shade(c, -0.22)], [1, shade(c, -0.5)]], 0.15 + ang, 0, 0.7 - ang, 1);
    body += `<path d="${d}" fill="url(#${id})"/>`;
  });
  return { defs, body };
}

/**
 * Рельеф камня: шум, освещённый слева сверху, накладывается умножением — бугры и выбоины.
 */
export function bumpFilter(id, freq = "0.012 0.02", amount = 0.35, seed = 3, scale = 4) {
  const lo = 1 - amount;
  return `<filter id="${id}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">
    <feTurbulence type="fractalNoise" baseFrequency="${freq}" numOctaves="4" seed="${seed}" result="n"/>
    <feDiffuseLighting in="n" surfaceScale="${scale}" diffuseConstant="1" lighting-color="#ffffff" result="l"><feDistantLight azimuth="225" elevation="40"/></feDiffuseLighting>
    <feComponentTransfer in="l" result="m"><feFuncR type="linear" slope="${amount * 1.6}" intercept="${lo}"/><feFuncG type="linear" slope="${amount * 1.6}" intercept="${lo}"/><feFuncB type="linear" slope="${amount * 1.6}" intercept="${lo}"/><feFuncA type="linear" slope="0" intercept="1"/></feComponentTransfer>
    <feBlend in="SourceGraphic" in2="m" mode="multiply" result="b"/>
    <feComposite in="b" in2="SourceGraphic" operator="in"/>
  </filter>`;
}

