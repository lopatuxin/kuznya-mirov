// Слои позади уровня: небо, облака, дальние горы, кремль на холме, дальний и ближний лес.
// Повторяющиеся по ширине слои замкнуты по краям: что выходит за край, дорисовано с другой стороны.
import { CELL, rng, smoothOpen, linear, radial, blurFilter, grainFilter, svg, shade, mix, num, blob } from "./brush.mjs";

/** Цвет дымки у горизонта: дальнее растворяется в нём. */
export const HAZE = "#cfe0e6";

/** Фильтр, который красит нарисованное белым: маска «где есть рисунок». */
const WHITE = `<filter id="white"><feColorMatrix type="matrix" values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 1 0"/></filter>`;

export function sky() {
  const W = 8 * CELL;
  const H = 12 * CELL;
  const defs = linear("sky", [[0, "#4f8fd3"], [0.35, "#7db1e3"], [0.62, "#b9d6ea"], [0.8, "#dbe8ea"], [1, "#e9eee6"]]);
  return { svg: svg(W, H, `<rect width="${W}" height="${H}" fill="url(#sky)"/>`, defs), W, H };
}

/** Кучевое облако: пушистые шапки сверху целиком в картинке, ровное дно с голубой тенью. */
export function cloud(seed, wCells, hCells) {
  const rnd = rng(seed);
  const W = Math.round(wCells * CELL);
  const H = Math.round(hCells * CELL);
  const base = H * 0.84;
  const n = Math.round(wCells * 1.5);
  const shapes = [];
  for (let i = 0; i < n; i++) {
    const t = (i + 0.5) / n;
    const hump = Math.sin(t * Math.PI);
    const x = W * (0.12 + 0.76 * t) + rnd.range(-6, 6);
    // Шапка не выше верха картинки и не шире её краёв.
    const r = Math.min((0.18 + 0.26 * hump) * H * rnd.range(0.75, 1.15), 0.46 * H, x - 4, W - x - 4);
    const cy = Math.max(r + 4, base - r * 0.62 - hump * H * 0.06 + rnd.range(-0.04, 0.04) * H);
    shapes.push({ cx: x, cy, r });
  }
  const circles = (fill) => shapes.map((c) => `<circle cx="${num(c.cx)}" cy="${num(c.cy)}" r="${num(c.r)}" fill="${fill}"/>`).join("");
  const defs = [
    blurFilter("cs", 1.5),
    radial("cp", [[0, "#ffffff"], [0.6, "#f8fafc"], [0.9, "#e2eaf2"], [1, "#d3dfeb"]], 0.38, 0.3, 0.75),
    linear("cshade", [[0, "#b9c9dc", 0], [0.62, "#b9c9dc", 0], [1, "#a9bcd3", 0.85]]),
    `<clipPath id="cflat"><rect width="${W}" height="${num(base)}"/></clipPath>`,
    `<mask id="cmask"><g clip-path="url(#cflat)">${circles("#fff")}</g></mask>`,
  ].join("");
  const body = `<g filter="url(#cs)"><g clip-path="url(#cflat)">${circles("url(#cp)")}</g>` +
    `<rect width="${W}" height="${H}" fill="url(#cshade)" mask="url(#cmask)"/></g>`;
  return { svg: svg(W, H, body, defs), W, H };
}

/** Линия хребта по ширине `W`, замкнутая по краям: сумма синусов с целым числом волн. */
function ridge(rnd, W, yBase, amp, waves) {
  const comps = waves.map((k) => ({ k, a: rnd.range(0.4, 1) / Math.sqrt(k), p: rnd.range(0, Math.PI * 2) }));
  const sum = comps.reduce((s, c) => s + c.a, 0);
  const pts = [];
  const steps = 96;
  for (let i = 0; i <= steps; i++) {
    const x = (W * i) / steps;
    let y = 0;
    for (const c of comps) y += c.a * Math.sin((x / W) * Math.PI * 2 * c.k + c.p);
    pts.push([x, yBase - (y / sum) * amp]);
  }
  return pts;
}

/** Дальние горы в дымке: два хребта, светлее к низу. */
export function mountains() {
  const rnd = rng(31);
  const W = 32 * CELL;
  const H = 6 * CELL;
  const far = ridge(rnd, W, H * 0.42, H * 0.3, [1, 2, 3, 5, 8, 13]);
  const near = ridge(rnd, W, H * 0.62, H * 0.24, [2, 3, 4, 7, 11, 17]);
  const path = (pts) => `${smoothOpen(pts, 1)}L${W},${H}L0,${H}Z`;
  const defs = [
    linear("mfar", [[0, "#9db4cf"], [1, HAZE]]),
    linear("mnear", [[0, "#89a6c0"], [0.7, "#bcd0dc"], [1, HAZE]]),
    grainFilter("mg", 0.5, 0.06, 4),
  ].join("");
  // Скальные осыпи светлыми полосами по склонам ближнего хребта.
  let streaks = "";
  for (let i = 0; i < 26; i++) {
    const x = rnd.range(0, W);
    const idx = Math.round((x / W) * 96);
    const y = near[idx][1] + rnd.range(6, 24);
    streaks += `<path d="M${num(x)},${num(y)} l${num(rnd.range(-30, 30))},${num(rnd.range(40, 90))}" stroke="#d6e1ea" stroke-width="${num(rnd.range(4, 9))}" stroke-linecap="round" opacity="0.5"/>`;
  }
  const body = `<g filter="url(#mg)"><path d="${path(far)}" fill="url(#mfar)"/><path d="${path(near)}" fill="url(#mnear)"/>${streaks}</g>`;
  return { svg: svg(W, H, body, defs), W, H };
}

/** Ёлка силуэтом: ярусы лап, верхушка острая. */
function spruce(rnd, x, yBottom, h, fill, light) {
  const w = h * rnd.range(0.28, 0.36);
  const tiers = Math.max(4, Math.round(h / 22));
  let left = [];
  let right = [];
  for (let i = 0; i <= tiers; i++) {
    const t = i / tiers;
    const y = yBottom - h * 0.08 - (h * 0.92) * t;
    const ww = (w / 2) * (1 - t) ** 0.95;
    left.push([x - ww * rnd.range(0.85, 1.1), y + h * 0.02], [x - ww * 0.45, y - h * 0.035]);
    right.push([x + ww * rnd.range(0.85, 1.1), y + h * 0.02], [x + ww * 0.45, y - h * 0.035]);
  }
  const top = [x, yBottom - h];
  const pts = [[x - w * 0.06, yBottom], ...left, top, ...right.reverse(), [x + w * 0.06, yBottom]];
  let s = `<path d="M${pts.map(([a, b]) => `${num(a)},${num(b)}`).join("L")}Z" fill="${fill}"/>`;
  if (light) {
    // Освещённый левый край лап.
    const lp = [top, ...left.slice().reverse().filter((_, i) => i % 2 === 1)];
    s += `<path d="M${lp.map(([a, b]) => `${num(a + 3)},${num(b + 2)}`).join("L")}" stroke="${light}" stroke-width="3" fill="none" opacity="0.6"/>`;
  }
  return s;
}

/** Берёза: тонкий белый ствол и крона из мелких гроздьев листвы. */
function birch(rnd, x, yBottom, h, crown, trunk) {
  const cw = h * rnd.range(0.3, 0.4);
  let s = `<path d="M${num(x - 3)},${num(yBottom)} L${num(x - 1.5)},${num(yBottom - h * 0.8)} L${num(x + 1.5)},${num(yBottom - h * 0.8)} L${num(x + 3)},${num(yBottom)}Z" fill="${trunk}"/>`;
  const dark = shade(crown, -0.18);
  for (let i = 0; i < 16; i++) {
    const cy = yBottom - h * rnd.range(0.45, 0.95);
    const cx = x + rnd.range(-cw * 0.45, cw * 0.45) * (1 - (yBottom - cy) / h * 0.4);
    const r = h * rnd.range(0.06, 0.1);
    s += `<path d="${blob(rnd, cx, cy, r, r * 0.8, 7, 0.3)}" fill="${i < 6 ? dark : crown}"/>`;
  }
  return s;
}

/**
 * Полоса леса по ширине `w` клеток и высоте `h`: деревья от `count`, цвета по дальности `far` 0..1.
 * Деревья у краёв дорисованы со сдвигом на ширину — слой повторяется без шва.
 */
export function forest({ w, h, seed, far, count, minH, maxH, birches = 0.25, groundFrom = 0.78 }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const sprC = mix("#26493a", HAZE, far * 0.78);
  const sprC2 = mix("#33603f", HAZE, far * 0.72);
  const sprL = mix("#7fa36a", HAZE, far * 0.6);
  const birC = [mix("#d9b84a", HAZE, far * 0.6), mix("#9fb84e", HAZE, far * 0.6), mix("#c7a23a", HAZE, far * 0.6)];
  const trunk = mix("#e9e6dc", HAZE, far * 0.5);
  const yGround = H * groundFrom;
  const trees = [];
  for (let i = 0; i < count; i++) {
    trees.push({ x: rnd.range(0, W), hh: rnd.range(minH, maxH) * CELL, b: rnd() < birches, y: yGround + rnd.range(-0.15, 0.3) * CELL });
  }
  trees.sort((a, b) => a.y - b.y || b.hh - a.hh);
  let body = "";
  // Подлесок полосой внизу, чтобы деревья стояли на чём-то.
  const under = ridge(rnd, W, yGround + 0.15 * CELL, 0.25 * CELL, [3, 5, 9, 14, 21]);
  body += `<path d="${smoothOpen(under, 1)}L${W},${H}L0,${H}Z" fill="${mix("#3f6b3c", HAZE, far * 0.7)}"/>`;
  for (const t of trees) {
    for (const dx of [-W, 0, W]) {
      const x = t.x + dx;
      if (x < -t.hh || x > W + t.hh) continue;
      body += t.b
        ? birch(rnd, x, t.y, t.hh * 0.85, rnd.pick(birC), trunk)
        : spruce(rnd, x, t.y, t.hh, rnd() < 0.5 ? sprC : sprC2, sprL);
    }
  }
  // Дымка к низу: лес уходит в туман у земли.
  const defs = linear("fhaze", [[0, HAZE, 0], [0.55, HAZE, 0.08], [1, HAZE, 0.75 - far * 0.3]]) + grainFilter("fg", 0.7, 0.07, seed) + WHITE +
    `<mask id="fm"><use href="#fc" filter="url(#white)"/></mask>`;
  body = `<g id="fc" filter="url(#fg)">${body}</g><rect width="${W}" height="${H}" fill="url(#fhaze)" mask="url(#fm)"/>`;
  return { svg: svg(W, H, body, defs), W, H };
}

/** Кремль на дальнем холме: стена с башнями, шатры и маковки, всё в дымке. */
export function kremlin() {
  const rnd = rng(77);
  const W = 10 * CELL;
  const H = 5 * CELL;
  const haze = (c, k = 0.45) => mix(c, HAZE, k);
  let body = "";
  // Холм.
  const hill = [[0, H], [W * 0.05, H * 0.86], [W * 0.25, H * 0.66], [W * 0.5, H * 0.6], [W * 0.75, H * 0.66], [W * 0.95, H * 0.84], [W, H]];
  body += `<path d="${smoothOpen(hill, 1)}Z" fill="${haze("#6f9461", 0.55)}"/>`;
  const wallTop = H * 0.52;
  const wallBot = H * 0.66;
  const x0 = W * 0.2;
  const x1 = W * 0.8;
  // Стена с зубцами.
  body += `<rect x="${num(x0)}" y="${num(wallTop)}" width="${num(x1 - x0)}" height="${num(wallBot - wallTop + 10)}" fill="${haze("#efe9dc")}"/>`;
  for (let x = x0; x < x1; x += 14) body += `<rect x="${num(x)}" y="${num(wallTop - 7)}" width="8" height="8" fill="${haze("#efe9dc")}"/>`;
  body += `<rect x="${num(x0)}" y="${num(wallTop + 12)}" width="${num(x1 - x0)}" height="4" fill="${haze("#c9c0ae")}"/>`;
  const roof = haze("#8a5a3c", 0.4);
  const dome = [haze("#c99a3a", 0.35), haze("#5d7ca8", 0.35), haze("#4f8a62", 0.35)];
  // Башня: прямоугольник, шатёр, маковка.
  const tower = (cx, w, top, tent) => {
    let s = `<rect x="${num(cx - w / 2)}" y="${num(top)}" width="${num(w)}" height="${num(wallBot - top + 10)}" fill="${haze("#f3eee3")}"/>`;
    s += `<rect x="${num(cx - w / 2)}" y="${num(top)}" width="${num(w * 0.32)}" height="${num(wallBot - top + 10)}" fill="${haze("#d9d0bd")}"/>`;
    for (let wy = top + 14; wy < wallBot - 10; wy += 22) s += `<rect x="${num(cx - 3)}" y="${num(wy)}" width="6" height="10" rx="3" fill="${haze("#6b6a72")}"/>`;
    s += `<path d="M${num(cx - w / 2 - 4)},${num(top)} L${num(cx)},${num(top - tent)} L${num(cx + w / 2 + 4)},${num(top)}Z" fill="${roof}"/>`;
    const dc = rnd.pick(dome);
    const dr = w * 0.22;
    s += `<path d="M${num(cx - dr)},${num(top - tent + 2)} Q${num(cx - dr * 1.2)},${num(top - tent - dr * 1.4)} ${num(cx)},${num(top - tent - dr * 2.4)} Q${num(cx + dr * 1.2)},${num(top - tent - dr * 1.4)} ${num(cx + dr)},${num(top - tent + 2)}Z" fill="${dc}"/>`;
    s += `<path d="M${num(cx)},${num(top - tent - dr * 2.4)} l0,-10" stroke="${haze("#c99a3a", 0.3)}" stroke-width="2"/>`;
    return s;
  };
  // Терем и собор позади стены.
  body += tower(W * 0.42, 34, H * 0.3, 40);
  body += tower(W * 0.55, 46, H * 0.22, 56);
  body += tower(W * 0.63, 30, H * 0.34, 34);
  // Угловые и надвратная башни поверх стены.
  body += tower(x0, 28, H * 0.42, 34);
  body += tower(x1, 28, H * 0.42, 34);
  body += tower(W * 0.33, 26, H * 0.45, 28);
  body += tower(W * 0.7, 24, H * 0.46, 26);
  // Деревья у подножия.
  for (let i = 0; i < 22; i++) {
    const x = rnd.range(W * 0.05, W * 0.95);
    body += spruce(rnd, x, H * rnd.range(0.7, 0.8), rnd.range(0.4, 0.9) * CELL, haze("#4a6f4c", 0.5), null);
  }
  return { svg: svg(W, H, body, ""), W, H };
}

