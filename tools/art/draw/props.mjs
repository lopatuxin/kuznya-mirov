// Предметы уровня: дно ущелья с ручьём, брёвна на верёвках, упавшее бревно, валун, светящиеся грибы,
// ворота, стволы позади и тёмная трава переднего плана.
import { CELL, rng, smoothClosed, smoothOpen, linear, radial, blurFilter, grainFilter, svg, shade, mix, num, blob, capsule } from "./brush.mjs";
import { rockFace, bumpFilter } from "./rock.mjs";
import { mossCap, tuft, fern, MOSS, MOSS_DEEP } from "./moss.mjs";
import { HAZE } from "./background.mjs";

const OUTLINE = "#2b2219";
const BARK = "#7a5634";
const BARK_DARK = "#4a321d";
const BARK_LIGHT = "#a87c4f";
const WOOD_END = "#c79a62";

/** Дно ущелья: ручей с порогами и валунами внизу, выше прозрачно — там виден фон. */
export function gorge({ w, seed }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(3.2 * CELL);
  const waterTop = 1.35 * CELL;
  let body = "";
  // Дальний берег: камни в тени.
  const shore = [];
  for (let x = -20; x <= W + 20; x += 40) shore.push([x, waterTop - rnd.range(0.1, 0.55) * CELL]);
  body += `<path d="${smoothOpen(shore, 1)}L${W + 20},${num(waterTop + 20)}L-20,${num(waterTop + 20)}Z" fill="${mix("#5d6a6c", HAZE, 0.25)}"/>`;
  // Вода: бирюзовая, светлее у порогов.
  body += `<rect x="-10" y="${num(waterTop)}" width="${W + 20}" height="${num(H - waterTop + 10)}" fill="url(#water)"/>`;
  for (let i = 0; i < w * 3; i++) {
    const x = rnd.range(0, W);
    const y = rnd.range(waterTop + 10, H - 10);
    const l = rnd.range(0.3, 1.0) * CELL;
    body += `<path d="M${num(x)},${num(y)} q${num(l / 2)},${num(rnd.range(-4, 4))} ${num(l)},0" stroke="#d7f1f4" stroke-width="${num(rnd.range(2, 4))}" stroke-linecap="round" fill="none" opacity="${num(rnd.range(0.35, 0.8))}"/>`;
  }
  // Валуны в воде с пеной вокруг.
  const n = Math.max(2, Math.round(w * 0.7));
  for (let i = 0; i < n; i++) {
    const cx = rnd.range(0.05, 0.95) * W;
    const cy = rnd.range(waterTop + 0.5 * CELL, H - 0.3 * CELL);
    const rx = rnd.range(0.25, 0.6) * CELL;
    const ry = rx * rnd.range(0.55, 0.8);
    body += `<ellipse cx="${num(cx)}" cy="${num(cy + ry * 0.55)}" rx="${num(rx * 1.35)}" ry="${num(ry * 0.45)}" fill="#f2fbfb" opacity="0.85"/>`;
    const id = `gb${seed}_${i}`;
    body += `<defs>${radial(id, [[0, "#b8bab3"], [0.6, "#7d8384"], [1, "#454c50"]], 0.35, 0.3, 0.8)}</defs>`;
    body += `<path d="${blob(rnd, cx, cy, rx, ry, 9, 0.15)}" fill="url(#${id})" stroke="#2c3236" stroke-width="2"/>`;
    body += `<path d="${blob(rnd, cx - rx * 0.15, cy - ry * 0.7, rx * 0.6, ry * 0.3, 7, 0.2)}" fill="${MOSS}" opacity="0.85"/>`;
  }
  const defs = linear("water", [[0, "#3e8aa0"], [0.4, "#4fa3b4"], [1, "#2f6f85"]]) + grainFilter("gg", 0.9, 0.06, seed);
  return { svg: svg(W, H, `<g filter="url(#gg)">${body}</g>`, defs), W, H };
}

/** Бревно сбоку: кора полосами, торцы с кольцами, мох сверху. */
function logBody(rnd, x0, x1, yc, r, idp) {
  let defs = linear(`${idp}bark`, [[0, BARK_LIGHT], [0.35, BARK], [1, BARK_DARK]]);
  let s = `<path d="${capsule([x0, yc], [x1, yc], r * 2, r * 2 * 0.95)}" fill="url(#${idp}bark)" stroke="${OUTLINE}" stroke-width="2.4"/>`;
  // Трещины коры вдоль.
  for (let i = 0; i < (x1 - x0) / 14; i++) {
    const x = rnd.range(x0 + r * 0.8, x1 - r * 0.6);
    const y = yc + rnd.range(-0.75, 0.75) * r;
    const l = rnd.range(20, 70);
    s += `<path d="M${num(x)},${num(y)} l${num(l)},${num(rnd.range(-2, 2))}" stroke="${BARK_DARK}" stroke-width="${num(rnd.range(1.5, 3))}" stroke-linecap="round" opacity="0.75"/>`;
  }
  // Торец справа: срез с кольцами.
  s += `<ellipse cx="${num(x1)}" cy="${num(yc)}" rx="${num(r * 0.42)}" ry="${num(r * 0.95)}" fill="${WOOD_END}" stroke="${OUTLINE}" stroke-width="2.2"/>`;
  for (let k = 0.3; k < 0.95; k += 0.22) s += `<ellipse cx="${num(x1)}" cy="${num(yc)}" rx="${num(r * 0.42 * k)}" ry="${num(r * 0.95 * k)}" fill="none" stroke="#9a7040" stroke-width="1.4"/>`;
  return { defs, s };
}

/** Бревно на двух верёвках, уходящих вверх за край экрана. */
export function hangingLog({ w, h, seed }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const r = 0.36 * CELL;
  const yc = H - r - 4;
  const x0 = 0.25 * CELL;
  const x1 = W - 0.3 * CELL;
  const log = logBody(rnd, x0, x1, yc, r, `hl${seed}`);
  let body = "";
  // Верёвки: витые, с намотками на бревне.
  const rope = (x) => {
    let s = `<path d="M${num(x - 2)},0 L${num(x + 2)},${num(yc - r + 4)}" stroke="#6e5232" stroke-width="7" stroke-linecap="round"/>`;
    s += `<path d="M${num(x - 2)},0 L${num(x + 2)},${num(yc - r + 4)}" stroke="#a88457" stroke-width="3" stroke-dasharray="5 6" stroke-linecap="round"/>`;
    for (let k = -1; k <= 1; k++) s += `<path d="M${num(x - 9 + k * 9)},${num(yc - r - 2)} q4,${num(r)} 0,${num(r * 2 + 4)}" stroke="#8a6a42" stroke-width="5" fill="none" stroke-linecap="round"/>`;
    return s;
  };
  body += log.s;
  const cap = mossCap(rnd, [[x0 + r * 0.6, yc - r + 2], [x1 - r * 0.3, yc - r + 2]], 0.2 * CELL, { idp: `hlm${seed}`, hang: 0.4 });
  body += cap.body;
  body += rope(x0 + 0.55 * CELL) + rope(x1 - 0.55 * CELL);
  return { svg: svg(W, H, body, log.defs + cap.defs), W, H };
}

/** Упавшее бревно во мху с грибами-трутовиками. */
export function fallenLog({ w, h, seed }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const r = 0.55 * CELL;
  const yc = H - r - 0.25 * CELL;
  const log = logBody(rnd, 0.3 * CELL, W - 0.4 * CELL, yc, r, `fl${seed}`);
  let body = log.s;
  // Сучья-обломки.
  body += `<path d="${capsule([W * 0.35, yc - r * 0.6], [W * 0.3, yc - r * 1.6], 14, 8)}" fill="${BARK}" stroke="${OUTLINE}" stroke-width="2"/>`;
  const cap = mossCap(rnd, [[0.6 * CELL, yc - r + 4], [W - 0.7 * CELL, yc - r + 4]], 0.26 * CELL, { idp: `flm${seed}`, hang: 0.6 });
  body += cap.body;
  // Трутовики полками на боку.
  for (let i = 0; i < 4; i++) {
    const x = rnd.range(0.2, 0.75) * W;
    const y = yc + rnd.range(-0.1, 0.5) * r;
    const fw = rnd.range(14, 24);
    body += `<path d="M${num(x - fw)},${num(y)} Q${num(x)},${num(y - fw * 0.7)} ${num(x + fw)},${num(y)} Q${num(x)},${num(y + 5)} ${num(x - fw)},${num(y)}Z" fill="#e3d9c4" stroke="${OUTLINE}" stroke-width="1.6"/>`;
    body += `<path d="M${num(x - fw * 0.7)},${num(y - 2)} Q${num(x)},${num(y - fw * 0.45)} ${num(x + fw * 0.7)},${num(y - 2)}" stroke="#b8a382" stroke-width="1.4" fill="none"/>`;
  }
  for (let i = 0; i < w * 1.5; i++) {
    const x = rnd.range(0.6 * CELL, W - 0.7 * CELL);
    body += tuft(rnd, x, cap.yAt(x) + 4, rnd.int(3, 6), rnd.range(0.1, 0.25) * CELL);
  }
  return { svg: svg(W, H, body, log.defs + cap.defs), W, H };
}

/** Валун во мху: верх ровный — на него встают. */
export function boulder({ w, h, seed }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const top = 0.3 * CELL;
  const pts = [
    [0.08 * W, H], [0.03 * W, H * 0.6], [0.12 * W, top + 0.2 * CELL], [0.3 * W, top + 4], [0.7 * W, top + 2],
    [0.88 * W, top + 0.15 * CELL], [0.97 * W, H * 0.55], [0.93 * W, H],
  ];
  const shape = smoothClosed(pts, 0.9);
  const rock = rockFace(rnd, [0, top, W, H], { size: 0.9 * CELL, idp: `bo${seed}_` });
  const cap = mossCap(rnd, [[0.2 * W, top - 6], [0.82 * W, top - 6]], 0.28 * CELL, { idp: `bom${seed}`, hang: 0.5 });
  let grass = "";
  for (let i = 0; i < w * 2; i++) {
    const x = rnd.range(0.22 * W, 0.8 * W);
    grass += tuft(rnd, x, cap.yAt(x) + 4, rnd.int(3, 7), rnd.range(0.12, 0.3) * CELL);
  }
  const defs = `<clipPath id="bc${seed}"><path d="${shape}"/></clipPath>` + rock.defs + cap.defs + bumpFilter(`bb${seed}`, "0.012 0.02", 0.4, seed, 6) +
    linear(`bd${seed}`, [[0, "#20262c", 0], [1, "#20262c", 0.7]]);
  const body = `<g filter="url(#bb${seed})"><path d="${shape}" fill="#474e58"/><g clip-path="url(#bc${seed})">${rock.body}<rect width="${W}" height="${H}" fill="url(#bd${seed})"/></g></g>` +
    `<path d="${shape}" fill="none" stroke="#252b33" stroke-width="2.5"/>` + cap.body + grass;
  return { svg: svg(W, H, body, defs), W, H };
}

/** Светящиеся грибы: бледные ножки, голубые шляпки. `glow` — отдельная картинка свечения. */
export function mushrooms({ seed, glow = false }) {
  const rnd = rng(seed);
  const W = Math.round(1.4 * CELL);
  const H = Math.round(1.0 * CELL);
  const items = [];
  for (let i = 0; i < 5; i++) {
    items.push({ x: rnd.range(0.2, 0.8) * W, h: rnd.range(0.25, 0.6) * H, r: rnd.range(0.07, 0.14) * W, lean: rnd.range(-6, 6) });
  }
  items.sort((a, b) => b.h - a.h);
  let body = "";
  let defs = "";
  if (glow) {
    defs = blurFilter("gl", 14);
    for (const m of items) body += `<ellipse cx="${num(m.x + m.lean)}" cy="${num(H - m.h)}" rx="${num(m.r * 2.4)}" ry="${num(m.r * 1.6)}" fill="#59d2ff" opacity="0.55"/>`;
    return { svg: svg(W, H, `<g filter="url(#gl)">${body}</g>`, defs), W, H };
  }
  defs = radial("mc", [[0, "#e9fdff"], [0.45, "#8fe6ff"], [1, "#3c9cd6"]], 0.4, 0.35, 0.75);
  for (const m of items) {
    const base = [m.x, H - 4];
    const capC = [m.x + m.lean, H - m.h];
    body += `<path d="${capsule(base, capC, 6, 4)}" fill="#eef4f0" stroke="#3a5560" stroke-width="1.4"/>`;
    body += `<path d="M${num(capC[0] - m.r)},${num(capC[1] + 2)} Q${num(capC[0] - m.r)},${num(capC[1] - m.r * 1.1)} ${num(capC[0])},${num(capC[1] - m.r * 1.1)} Q${num(capC[0] + m.r)},${num(capC[1] - m.r * 1.1)} ${num(capC[0] + m.r)},${num(capC[1] + 2)}Z" fill="url(#mc)" stroke="#2f6f96" stroke-width="1.4"/>`;
  }
  // Мох у основания.
  body += `<path d="${blob(rnd, W / 2, H - 4, W * 0.42, 8, 9, 0.2)}" fill="${MOSS_DEEP}"/>`;
  return { svg: svg(W, H, body, defs), W, H };
}

/** Ворота конца уровня: два столба, перекладина, двускатная крыша с резными причелинами и солнцем. */
export function gate() {
  const W = Math.round(4.6 * CELL);
  const H = Math.round(5.4 * CELL);
  const rnd = rng(91);
  const wood = "#a0703e";
  const woodDark = "#64421f";
  let defs = linear("post", [[0, shade(wood, 0.15)], [0.5, wood], [1, woodDark]], 0, 0, 1, 0);
  defs += linear("roof", [[0, "#8a6a48"], [1, "#4f3a26"]]);
  let body = "";
  const postW = 0.42 * CELL;
  const px = [0.75 * CELL, W - 0.75 * CELL];
  const postTop = 1.5 * CELL;
  for (const x of px) {
    body += `<rect x="${num(x - postW / 2)}" y="${num(postTop)}" width="${num(postW)}" height="${num(H - postTop)}" fill="url(#post)" stroke="${OUTLINE}" stroke-width="2.4"/>`;
    for (let y = postTop + 30; y < H - 20; y += rnd.range(40, 70)) body += `<path d="M${num(x - postW / 2 + 4)},${num(y)} l${num(postW - 8)},${num(rnd.range(-3, 3))}" stroke="${woodDark}" stroke-width="1.6" opacity="0.7"/>`;
  }
  // Перекладина.
  body += `<rect x="${num(px[0] - postW * 0.7)}" y="${num(postTop + 0.15 * CELL)}" width="${num(px[1] - px[0] + postW * 1.4)}" height="${num(0.32 * CELL)}" fill="url(#post)" stroke="${OUTLINE}" stroke-width="2.4"/>`;
  // Крыша.
  const ridgeY = 0.25 * CELL;
  const eaveY = postTop + 0.05 * CELL;
  body += `<path d="M${num(-6)},${num(eaveY)} L${num(W / 2)},${num(ridgeY)} L${num(W + 6)},${num(eaveY)} L${num(W - 0.15 * CELL)},${num(eaveY + 0.2 * CELL)} L${num(W / 2)},${num(ridgeY + 0.3 * CELL)} L${num(0.15 * CELL)},${num(eaveY + 0.2 * CELL)}Z" fill="url(#roof)" stroke="${OUTLINE}" stroke-width="2.4"/>`;
  // Тёс полосами.
  for (let k = 0.1; k < 1; k += 0.12) {
    const xl = W / 2 - (W / 2) * k;
    const xr = W / 2 + (W / 2) * k;
    const y = ridgeY + (eaveY - ridgeY) * k;
    body += `<path d="M${num(xl)},${num(y)} l0,${num(0.2 * CELL)} M${num(xr)},${num(y)} l0,${num(0.2 * CELL)}" stroke="#3c2b1b" stroke-width="1.6"/>`;
  }
  // Причелины с зубцами и солнце во фронтоне.
  for (const s of [-1, 1]) {
    const pts = [];
    for (let k = 0; k <= 10; k++) {
      const t = k / 10;
      const x = W / 2 + s * (W / 2 - 0.12 * CELL) * t;
      const y = ridgeY + 0.3 * CELL + (eaveY - ridgeY) * t;
      pts.push(`${num(x)},${num(y)}`, `${num(x + s * 8)},${num(y + 12)}`);
    }
    body += `<path d="M${pts.join("L")}" stroke="#d8b37a" stroke-width="3" fill="none" stroke-linejoin="round"/>`;
  }
  const sun = [W / 2, ridgeY + 0.75 * CELL];
  body += `<circle cx="${num(sun[0])}" cy="${num(sun[1])}" r="${num(0.22 * CELL)}" fill="#e6b44c" stroke="${OUTLINE}" stroke-width="2"/>`;
  for (let i = 0; i < 12; i++) {
    const a = (i / 12) * Math.PI * 2;
    body += `<path d="M${num(sun[0] + Math.cos(a) * 0.26 * CELL)},${num(sun[1] + Math.sin(a) * 0.26 * CELL)} L${num(sun[0] + Math.cos(a) * 0.36 * CELL)},${num(sun[1] + Math.sin(a) * 0.36 * CELL)}" stroke="#e6b44c" stroke-width="3" stroke-linecap="round"/>`;
  }
  // Мох у столбов.
  for (const x of px) body += `<path d="${blob(rnd, x, H - 6, postW * 1.1, 12, 9, 0.25)}" fill="${MOSS}"/>` + tuft(rnd, x, H - 4, 8, 0.3 * CELL);
  return { svg: svg(W, H, body, defs), W, H };
}

/** Тёмная трава и папоротник переднего плана: силуэты кучками, слой повторяется по ширине. */
export function foreground({ w, h, seed }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const dark = ["#0d1610", "#111d14", "#152419"];
  let body = "";
  const clumps = Math.round(w * 0.5);
  for (let i = 0; i < clumps; i++) {
    const x0 = rnd.range(0, W);
    const size = rnd.range(0.6, 1.4) * H;
    for (const dx of [-W, 0, W]) {
      const x = x0 + dx;
      if (x < -size || x > W + size) continue;
      const r2 = rng(seed * 100 + i);
      const fill = r2.pick(dark);
      for (let j = 0; j < 14; j++) {
        const bh = r2.range(0.3, 1) * size * 0.8;
        const lean = r2.range(-0.6, 0.6) * bh;
        const bx = x + r2.range(-0.3, 0.3) * size;
        const bw = r2.range(4, 10);
        body += `<path d="M${num(bx - bw)},${H + 4} Q${num(bx + lean * 0.3)},${num(H - bh * 0.6)} ${num(bx + lean)},${num(H - bh)} Q${num(bx + lean * 0.2 + bw * 0.5)},${num(H - bh * 0.5)} ${num(bx + bw)},${H + 4}Z" fill="${fill}"/>`;
      }
      if (r2() < 0.6) body += fern(r2, x + r2.range(-20, 20), H + 4, size * 0.75, r2() < 0.5 ? -1 : 1, fill).replace(/fill="#[0-9a-f]{6}"/g, `fill="${fill}"`).replace(/stroke="#[0-9a-f]{6}"/g, `stroke="${fill}"`);
    }
  }
  return { svg: svg(W, H, body, ""), W, H };
}

