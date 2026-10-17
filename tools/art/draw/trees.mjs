// Деревья позади уровня: ель ярусами лап, сосна с рыжим стволом и кроной наверху, плакучая берёза.
import { CELL, rng, linear, svg, shade, num, blob, capsule } from "./brush.mjs";

const NEEDLE_DARK = "#1f3b2a";
const NEEDLE = "#2e573a";
const NEEDLE_LIGHT = "#5f8a4f";

/** Ярус лап ели: от ствола вниз-вбок, нижний край зубчатый. */
function tier(rnd, cx, y, w, droop, thick, side, idp) {
  const tip = [cx + side * w, y + droop];
  const pts = [[cx, y - thick * 0.2], [cx + side * w * 0.5, y + droop * 0.15 - thick * 0.15], tip];
  const teeth = Math.max(4, Math.round(w / 14));
  for (let i = 1; i <= teeth; i++) {
    const t = i / teeth;
    const x = tip[0] - side * w * t;
    const yb = tip[1] + thick * (0.3 + 0.7 * Math.sin(t * Math.PI * 0.5)) - droop * t * 0.75;
    pts.push([x + side * rnd.range(-3, 3), yb + (i % 2 ? rnd.range(4, 9) : 0)]);
  }
  pts.push([cx, y + thick]);
  const id = `${idp}`;
  const def = linear(id, [[0, NEEDLE_LIGHT], [0.35, NEEDLE], [1, NEEDLE_DARK]], 0, 0, 0, 1);
  let s = `<path d="M${pts.map(([a, b]) => `${num(a)},${num(b)}`).join("L")}Z" fill="url(#${id})"/>`;
  // Хвоя штрихами по нижнему краю.
  for (let i = 0; i < teeth * 1.5; i++) {
    const t = rnd.range(0.1, 0.95);
    const x = cx + side * w * t;
    const yy = y + droop * t + thick * rnd.range(0.3, 0.8);
    s += `<path d="M${num(x)},${num(yy)} l${num(side * rnd.range(-4, 4))},${num(rnd.range(5, 11))}" stroke="${NEEDLE_DARK}" stroke-width="2" stroke-linecap="round"/>`;
  }
  // Свет слева сверху по верху яруса.
  if (side < 0) s += `<path d="M${num(cx - 4)},${num(y)} Q${num(cx + side * w * 0.5)},${num(y + droop * 0.1)} ${num(tip[0] + 6)},${num(tip[1] - 2)}" stroke="${shade(NEEDLE_LIGHT, 0.15)}" stroke-width="3" fill="none" opacity="0.7"/>`;
  return { def, s };
}

/** Большая ель позади уровня: верхушка за краем экрана, нижние лапы до земли. */
export function bigSpruce({ seed, w = 4.4, h = 10.5 }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const cx = W / 2;
  let defs = linear(`st${seed}`, [[0, "#7a5236"], [1, "#3e2818"]], 0, 0, 1, 0);
  let body = `<path d="${capsule([cx, -10], [cx, H], 10, 26)}" fill="url(#st${seed})"/>`;
  const tiers = [];
  let y = -0.4 * CELL;
  const bottom = H - 1.6 * CELL;
  while (y < bottom) {
    const t = (y + 0.4 * CELL) / (bottom + 0.4 * CELL);
    tiers.push({ y, w: (0.25 + 0.75 * t) * (W / 2 - 6) * rnd.range(0.85, 1.05), t });
    y += rnd.range(0.32, 0.45) * CELL;
  }
  // Снизу вверх: верхние ярусы лежат поверх нижних.
  tiers.reverse().forEach((tr, i) => {
    for (const side of [1, -1]) {
      const r = tier(rnd, cx, tr.y, tr.w, 0.35 * CELL * (0.5 + tr.t), 0.22 * CELL * (0.6 + tr.t * 0.6), side, `sp${seed}_${i}_${side}`);
      defs += r.def;
      body += r.s;
    }
  });
  return { svg: svg(W, H, body, defs), W, H };
}

/** Сосна: рыжий ствол во весь рост, сучья и пышные лапы хвои наверху. */
export function pine({ seed, w = 3.6, h = 10.5 }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const cx = W / 2;
  const wb = 0.8 * CELL;
  const wt = 0.45 * CELL;
  let defs = linear(`pt${seed}`, [[0, "#d08a5a"], [0.35, "#a8613b"], [1, "#5a3220"]], 0, 0, 1, 0);
  let body = "";
  // Ветви к кронам.
  const clumps = [];
  for (let i = 0; i < 6; i++) {
    const y = rnd.range(-0.2, 2.6) * CELL;
    const side = i % 2 ? -1 : 1;
    const end = [cx + side * rnd.range(0.6, 1.5) * CELL, y - rnd.range(0.1, 0.5) * CELL];
    body += `<path d="${capsule([cx, y + 0.3 * CELL], end, 12, 5)}" fill="#6e3f25"/>`;
    clumps.push(end);
  }
  clumps.push([cx, -0.2 * CELL]);
  body += `<path d="M${num(cx - wt / 2)},-10 L${num(cx + wt / 2)},-10 L${num(cx + wb / 2)},${H} L${num(cx - wb / 2)},${H}Z" fill="url(#pt${seed})"/>`;
  for (let y = 10; y < H; y += rnd.range(16, 30)) {
    const w2 = wt + (wb - wt) * (y / H);
    for (let k = 0; k < 2; k++) {
      const x = cx - w2 / 2 + rnd.range(0.05, 0.8) * w2;
      body += `<path d="M${num(x)},${num(y)} l${num(rnd.range(6, 18))},${num(rnd.range(-2, 2))}" stroke="#5e3320" stroke-width="${num(rnd.range(2, 4))}" stroke-linecap="round" opacity="0.8"/>`;
    }
  }
  // Лапы хвои облаками: тёмный низ, светлый верх.
  for (const [x, y] of clumps) {
    for (let k = 0; k < 4; k++) {
      const px = x + rnd.range(-0.4, 0.4) * CELL;
      const py = y + rnd.range(-0.25, 0.2) * CELL;
      const rx = rnd.range(0.4, 0.7) * CELL;
      const ry = rx * rnd.range(0.4, 0.55);
      body += `<path d="${blob(rnd, px, py + 5, rx, ry, 11, 0.22)}" fill="${NEEDLE_DARK}"/>`;
      body += `<path d="${blob(rnd, px - 4, py - 3, rx * 0.85, ry * 0.7, 11, 0.22)}" fill="${NEEDLE}"/>`;
      body += `<path d="${blob(rnd, px - rx * 0.25, py - ry * 0.45, rx * 0.45, ry * 0.3, 9, 0.25)}" fill="${NEEDLE_LIGHT}" opacity="0.8"/>`;
    }
  }
  return { svg: svg(W, H, body, defs), W, H };
}

/** Плакучая берёза: белый ствол, тонкие свисающие ветви с жёлто-зелёной листвой. */
export function weepingBirch({ seed, w = 3.4, h = 10.5 }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const cx = W / 2;
  const wb = 0.48 * CELL;
  const wt = 0.3 * CELL;
  const leaves = ["#d8b84a", "#b8c254", "#93ad48", "#c9a03c", "#e1c86a"];
  let defs = linear(`bt${seed}`, [[0, "#ffffff"], [0.45, "#ebe8df"], [1, "#a29e95"]], 0, 0, 1, 0);
  let back = "";
  let front = "";
  // Ветви: от ствола вверх-вбок, потом свисают вниз; листья гроздьями вдоль свисающей части.
  for (let i = 0; i < 9; i++) {
    const y0 = rnd.range(0.0, 4.2) * CELL;
    const side = i % 2 ? -1 : 1;
    const out = rnd.range(0.5, 1.4) * CELL;
    const p1 = [cx + side * out * 0.6, y0 - rnd.range(0.2, 0.6) * CELL];
    const p2 = [cx + side * out, y0 + rnd.range(0.4, 1.6) * CELL];
    const branch = `<path d="M${num(cx)},${num(y0)} Q${num(p1[0])},${num(p1[1])} ${num(p2[0])},${num(p2[1])}" stroke="#3b3430" stroke-width="3" fill="none"/>`;
    let leafs = "";
    for (let k = 0; k < 14; k++) {
      const t = rnd.range(0.35, 1);
      const x = (1 - t) * (1 - t) * cx + 2 * (1 - t) * t * p1[0] + t * t * p2[0];
      const y = (1 - t) * (1 - t) * y0 + 2 * (1 - t) * t * p1[1] + t * t * p2[1];
      leafs += `<path d="${blob(rnd, x + rnd.range(-10, 10), y + rnd.range(-6, 12), rnd.range(8, 15), rnd.range(6, 10), 7, 0.3)}" fill="${rnd.pick(leaves)}"/>`;
    }
    if (i < 4) back += branch + leafs;
    else front += branch + leafs;
  }
  let trunk = `<path d="M${num(cx - wt / 2)},-10 L${num(cx + wt / 2)},-10 L${num(cx + wb / 2)},${H} L${num(cx - wb / 2)},${H}Z" fill="url(#bt${seed})" stroke="#5b5650" stroke-width="2"/>`;
  for (let y = 12; y < H; y += rnd.range(16, 40)) {
    const w2 = wt + (wb - wt) * (y / H);
    const x = cx - w2 / 2 + rnd.range(0, 0.6) * w2;
    trunk += `<path d="M${num(x)},${num(y)} l${num(rnd.range(6, w2 * 0.5))},${num(rnd.range(-1, 1))}" stroke="#1f1c1a" stroke-width="${num(rnd.range(2, 4.5))}" stroke-linecap="round"/>`;
  }
  return { svg: svg(W, H, back + trunk + front, defs), W, H };
}

