// Скальный уступ во мху: верх — где стоит герой, обрывы по краям, ниже — камень до края экрана.
import { CELL, rng, smoothClosed, smoothOpen, linear, blurFilter, svg, num } from "./brush.mjs";
import { rockFace, bumpFilter } from "./rock.mjs";
import { mossCap, strands, tuft, fern, flower } from "./moss.mjs";

const ROCK_DARK = "#343b47";

/** Пятна мха на камне ниже шапки — гуще наверху. */
function mossPatches(rnd, x0, x1, T, blur) {
  let s = `<g opacity="0.5" filter="url(#${blur})">`;
  const n = Math.round(((x1 - x0) / CELL) * 1.2);
  for (let i = 0; i < n; i++) {
    const x = rnd.range(x0, x1);
    const y = T + rnd.range(0.45, 1.6) * CELL * (rnd() < 0.7 ? 1 : 1.6);
    const rx = rnd.range(0.15, 0.45) * CELL;
    const c = rnd.pick(["#7c9734", "#6a8a2e", "#93a947"]);
    s += `<ellipse cx="${num(x)}" cy="${num(y)}" rx="${num(rx)}" ry="${num(rx * rnd.range(0.3, 0.55))}" fill="${c}"/>`;
  }
  return s + "</g>";
}

/**
 * Уступ шириной `w` и высотой `h` клеток. Контур, по которому ходят, — на `top` клеток ниже верха
 * картинки и от `pad` клеток до `w - pad` по ширине. `left`/`right` — обрыв с этой стороны.
 */
export function ledge({ w, h, top = 0.45, pad = 0.35, left = true, right = true, seed = 1, ferns = 1, flowers = 1 }) {
  const rnd = rng(seed);
  const W = Math.round(w * CELL);
  const H = Math.round(h * CELL);
  const T = top * CELL;
  const x0 = left ? pad * CELL : -30;
  const x1 = right ? W - pad * CELL : W + 30;

  // Силуэт: верхняя кромка ровная (по ней идёт контур), обрывы с выступами глыб.
  const topLine = [];
  const steps = Math.max(3, Math.round((x1 - x0) / 90));
  for (let i = 0; i <= steps; i++) topLine.push([x0 + ((x1 - x0) * i) / steps, T + 8]);
  const cliff = (side) => {
    const pts = [];
    const baseX = side < 0 ? x0 : x1;
    let y = T + 0.3 * CELL;
    let k = 0;
    while (y < H + 60) {
      const bulge = (k % 2 === 0 ? rnd.range(0.05, 0.3) : rnd.range(-0.2, 0.0)) * CELL;
      const inward = Math.min(0.35 * CELL, (y - T) * 0.07);
      pts.push([baseX + side * (bulge - inward), y]);
      y += rnd.range(0.4, 0.75) * CELL;
      k++;
    }
    return pts;
  };
  const outline = [];
  if (left) outline.push(...cliff(-1).reverse(), [x0 + 0.05 * CELL, T + 0.08 * CELL]);
  else outline.push([-40, H + 60], [-40, T + 8]);
  outline.push(...topLine.slice(left ? 1 : 0, right ? -1 : undefined));
  if (right) outline.push([x1 - 0.05 * CELL, T + 0.08 * CELL], ...cliff(1));
  else outline.push([W + 40, T + 8], [W + 40, H + 60]);
  const body = smoothClosed(outline, 0.85);

  const rock = rockFace(rnd, [-40, T, W + 40, H + 40], { size: 1.35 * CELL, idp: `r${seed}_`, grow: 0.3 });
  const cap = mossCap(rnd, topLine.map(([x, y]) => [x, y - 10]), 0.46 * CELL, { idp: `m${seed}_`, hang: 0.5 });

  // Пряди мха с краёв обрыва.
  let hang = "";
  if (left) hang += strands(rnd, x0 + 0.1 * CELL, T + 0.25 * CELL, 7, -1, 0.9 * CELL);
  if (right) hang += strands(rnd, x1 - 0.1 * CELL, T + 0.25 * CELL, 7, 1, 0.9 * CELL);

  // Трава, папоротник, цветы по верху.
  let top2 = "";
  const span = (x1 - x0) / CELL;
  for (let i = 0; i < span * 2.2; i++) {
    const tx = rnd.range(x0 + 10, x1 - 10);
    top2 += tuft(rnd, tx, cap.yAt(tx) + 6, rnd.int(5, 10), rnd.range(0.18, 0.42) * CELL);
  }
  for (let i = 0; i < span * 0.25 * ferns; i++) {
    const tx = rnd.range(x0 + 20, x1 - 20);
    const fy = cap.yAt(tx) + 12;
    for (let k = 0; k < 5; k++) top2 += fern(rnd, tx + rnd.range(-6, 6), fy, rnd.range(0.4, 0.75) * CELL, k % 2 ? -1 : 1);
  }
  for (let i = 0; i < span * 0.9 * flowers; i++) {
    const tx = rnd.range(x0 + 10, x1 - 10);
    top2 += flower(tx, cap.yAt(tx) - rnd.range(2, 16), rnd.range(2.4, 3.6));
  }

  const defs = [
    `<clipPath id="body${seed}"><path d="${body}"/></clipPath>`,
    linear(`depth${seed}`, [[0, ROCK_DARK, 0], [0.25, ROCK_DARK, 0.1], [1, ROCK_DARK, 0.8]], 0, 0, 0, 1),
    linear(`sides${seed}`, [[0, ROCK_DARK, 0], [0.75, ROCK_DARK, 0], [1, ROCK_DARK, 0.45]], 0, 0, 1, 0),
    bumpFilter(`bump${seed}`, "0.006 0.012", 0.45, seed, 9),
    blurFilter(`soft${seed}`, 7),
    rock.defs,
    cap.defs,
  ].join("");

  const content = [
    `<g filter="url(#bump${seed})">`,
    `<path d="${body}" fill="#474e58"/>`,
    `<g clip-path="url(#body${seed})">`,
    rock.body,
    mossPatches(rnd, x0, x1, T, `soft${seed}`),
    `<path d="${smoothOpen(topLine.map(([x, y]) => [x, y + 0.36 * CELL]), 1)}" stroke="#141a12" stroke-width="${num(0.3 * CELL)}" opacity="0.4" fill="none" filter="url(#soft${seed})"/>`,
    `<rect x="-40" y="${num(T)}" width="${W + 80}" height="${num(H - T + 60)}" fill="url(#depth${seed})"/>`,
    right ? `<rect x="-40" y="0" width="${W + 80}" height="${H}" fill="url(#sides${seed})"/>` : "",
    `</g></g>`,
    `<path d="${body}" fill="none" stroke="#252b33" stroke-width="2.5" opacity="0.65"/>`,
    hang,
    cap.body,
    top2,
  ].join("");
  return { svg: svg(W, H, content, defs), W, H };
}

