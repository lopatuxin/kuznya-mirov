// Мох подушками, свисающие пряди, трава, папоротник и цветы — общий покров уступов, брёвен и камней.
import { radial, linear, smoothClosed, shade, num } from "./brush.mjs";

const MOSS_LIGHT = "#c9d05c";
export const MOSS = "#88a338";
export const MOSS_DEEP = "#4c6a23";
const GRASS = ["#a3bd47", "#88aa3b", "#6e9431", "#bccd5e", "#5d8229", "#97b443"];

let gradCount = 0;

/**
 * Шапка мха по линии `line` (точки слева направо): подушки, у каждой свет слева сверху.
 * `thick` — толщина шапки в точках. Возвращает `defs`, `body` и нижнюю кромку для тени.
 */
export function mossCap(rnd, line, thick, { idp = "m", hang = 0.15 } = {}) {
  let defs = "";
  const cushions = [];
  const xa = line[0][0];
  const xb = line[line.length - 1][0];
  const yAt = (x) => {
    for (let i = 0; i < line.length - 1; i++) {
      const [ax, ay] = line[i];
      const [bx, by] = line[i + 1];
      if (x >= ax && x <= bx) return ay + ((by - ay) * (x - ax)) / Math.max(1e-6, bx - ax);
    }
    return x < xa ? line[0][1] : line[line.length - 1][1];
  };
  // Нижний ряд — крупные тёмные, верхний — мелкие светлые.
  for (const [rowK, rowShade] of [[1.0, -0.12], [0.62, 0.0], [0.4, 0.1]]) {
    let x = xa + rnd.range(0, 14);
    while (x < xb - 4) {
      const rx = rnd.range(0.55, 1.05) * thick * rowK * 1.3;
      const ry = rx * rnd.range(0.45, 0.62);
      const cx = Math.min(xb - rx * 0.5, Math.max(xa + rx * 0.5, x + rx * 0.5));
      const top = yAt(cx);
      const cy = top + ry * (rowK === 1 ? 1.25 : 0.95) + (1 - rowK) * thick * 0.05;
      cushions.push({ cx, cy, rx, ry, tone: rowShade + rnd.range(-0.06, 0.06) });
      x += rx * rnd.range(0.9, 1.4);
    }
  }
  let body = "";
  for (const c of cushions) {
    const id = `${idp}${gradCount++}`;
    defs += radial(id, [[0, shade(MOSS_LIGHT, 0.12 + c.tone)], [0.45, shade(MOSS, c.tone)], [1, shade(MOSS_DEEP, c.tone)]], 0.38, 0.3, 0.75);
    body += `<path d="${smoothClosed(blobPts(rnd, c.cx, c.cy, c.rx, c.ry), 1)}" fill="url(#${id})"/>`;
  }
  // Подтёки мха вниз по камню.
  let drapes = "";
  const count = Math.round(((xb - xa) / 100) * hang * 4);
  for (let i = 0; i < count; i++) {
    const x = rnd.range(xa + 10, xb - 10);
    const y = yAt(x) + thick * 0.8;
    const len = rnd.range(0.4, 1.2) * thick;
    const w = rnd.range(0.35, 0.7) * thick;
    const id = `${idp}d${gradCount++}`;
    defs += linear(id, [[0, MOSS], [1, MOSS_DEEP]], 0, 0, 0, 1);
    drapes += `<path d="M${num(x - w)},${num(y - 4)} C${num(x - w)},${num(y + len * 0.6)} ${num(x - w * 0.6)},${num(y + len)} ${num(x)},${num(y + len)} C${num(x + w * 0.6)},${num(y + len)} ${num(x + w)},${num(y + len * 0.6)} ${num(x + w)},${num(y - 4)}Z" fill="url(#${id})"/>`;
  }
  return { defs, body: drapes + body, cushions, yAt };
}

function blobPts(rnd, cx, cy, rx, ry) {
  const pts = [];
  const n = 8;
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2;
    const k = 1 + rnd.range(-0.1, 0.1);
    pts.push([cx + Math.cos(a) * rx * k, cy + Math.sin(a) * ry * k]);
  }
  return pts;
}

/** Свисающие с обрыва пряди мха. */
export function strands(rnd, x, y, count, side, len = 60) {
  let s = "";
  for (let i = 0; i < count; i++) {
    const sx = x + rnd.range(-12, 12);
    const sy = y + rnd.range(-4, 10);
    const l = rnd.range(0.4, 1.1) * len;
    const sw = side * rnd.range(1, 8);
    const w = rnd.range(2.5, 5.5);
    s += `<path d="M${num(sx - w)},${num(sy)} Q${num(sx + sw - w * 0.4)},${num(sy + l * 0.55)} ${num(sx + sw * 0.5)},${num(sy + l)} Q${num(sx + sw + w * 0.4)},${num(sy + l * 0.5)} ${num(sx + w)},${num(sy)}Z" fill="${rnd.pick([MOSS, MOSS_DEEP, "#6c8c2d"])}"/>`;
  }
  return s;
}

/** Пучок травы у точки `x`, `y`: `n` травинок высотой до `h`. */
export function tuft(rnd, x, y, n, h) {
  let s = "";
  for (let j = 0; j < n; j++) {
    const hgt = rnd.range(0.35, 1) * h;
    const lean = rnd.range(-0.55, 0.55) * hgt;
    const bx = x + rnd.range(-0.25, 0.25) * h;
    const w = rnd.range(1.8, 3.6);
    s += `<path d="M${num(bx - w)},${num(y)} Q${num(bx + lean * 0.25)},${num(y - hgt * 0.65)} ${num(bx + lean)},${num(y - hgt)} Q${num(bx + lean * 0.15 + w * 0.4)},${num(y - hgt * 0.5)} ${num(bx + w)},${num(y)}Z" fill="${rnd.pick(GRASS)}"/>`;
  }
  return s;
}

/** Веточка папоротника: стебель дугой и листочки парами. */
export function fern(rnd, x, y, len, dir = 1, color = "#6f9a33") {
  const tipx = x + dir * len * rnd.range(0.35, 0.6);
  const tipy = y - len * rnd.range(0.55, 0.8);
  const cx = x + dir * len * 0.05;
  const cy = y - len * 0.75;
  let s = `<path d="M${num(x)},${num(y)} Q${num(cx)},${num(cy)} ${num(tipx)},${num(tipy)}" stroke="${shade(color, -0.2)}" stroke-width="2.2" fill="none"/>`;
  const n = 9;
  for (let i = 1; i < n; i++) {
    const t = i / n;
    const px = (1 - t) * (1 - t) * x + 2 * (1 - t) * t * cx + t * t * tipx;
    const py = (1 - t) * (1 - t) * y + 2 * (1 - t) * t * cy + t * t * tipy;
    const l = len * 0.22 * (1 - t * 0.75);
    for (const side of [-1, 1]) {
      const ex = px + side * l * 0.8 + dir * l * 0.3;
      const ey = py - l * 0.35 + side * l * 0.15;
      s += `<path d="M${num(px)},${num(py)} Q${num((px + ex) / 2)},${num(Math.min(py, ey) - l * 0.25)} ${num(ex)},${num(ey)} Q${num((px + ex) / 2)},${num(Math.max(py, ey) - l * 0.05)} ${num(px)},${num(py)}Z" fill="${shade(color, (side > 0 ? 0.08 : -0.04) + rnd.range(-0.05, 0.05))}"/>`;
    }
  }
  return s;
}

/** Белый лесной цветок. */
export function flower(x, y, r = 3.2) {
  let s = "";
  for (let i = 0; i < 5; i++) {
    const a = (i / 5) * Math.PI * 2;
    s += `<circle cx="${num(x + Math.cos(a) * r * 0.8)}" cy="${num(y + Math.sin(a) * r * 0.8)}" r="${num(r * 0.62)}" fill="#f6f3e8"/>`;
  }
  return s + `<circle cx="${num(x)}" cy="${num(y)}" r="${num(r * 0.45)}" fill="#e9bf3c"/>`;
}
