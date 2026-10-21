// Вспышки боя: искры парирования и вспышка удара. Светящиеся картинки на прозрачном фоне, кадры
// по строке; кадр выбирает код игры по прожитому вспышкой времени.
import { svg, rng, radial, num } from "./brush.mjs";

const FX = 128;
const MID = FX / 2;

const ray = (a, r0, r1, w, color, opacity = 1) => {
  const c = Math.cos(a);
  const s = Math.sin(a);
  const n = [-s * w, c * w];
  const p0 = [MID + c * r0 + n[0], MID + s * r0 + n[1]];
  const p1 = [MID + c * r0 - n[0], MID + s * r0 - n[1]];
  const tip = [MID + c * r1, MID + s * r1];
  return `<path d="M${num(p0[0])},${num(p0[1])}L${num(tip[0])},${num(tip[1])}L${num(p1[0])},${num(p1[1])}Z" fill="${color}" opacity="${opacity}"/>`;
};

const core = (id, r, opacity = 1) =>
  `<circle cx="${MID}" cy="${MID}" r="${r}" fill="url(#${id})" opacity="${opacity}"/>`;

/** Искры парирования: вспышка встречи, лучи разлетаются и рассыпаются точками. 4 кадра. */
export function clashSvg() {
  const rnd = rng(81);
  const rays = Array.from({ length: 13 }, (_, i) => ({
    a: (i / 13) * Math.PI * 2 + rnd.range(-0.18, 0.18),
    len: rnd.range(0.7, 1.15),
  }));
  const defs = radial("clashCore", [[0, "#ffffff"], [0.35, "#fff1a8"], [1, "#ff9a2a", 0]]);
  const frames = [
    () => core("clashCore", 30) + rays.map((r) => ray(r.a, 6, 46 * r.len, 3.2, "#fff6c8")).join(""),
    () => core("clashCore", 18, 0.8) + rays.map((r) => ray(r.a, 18 * r.len, 60 * r.len, 2.2, "#ffd36a")).join(""),
    () => core("clashCore", 8, 0.5) + rays.map((r) => ray(r.a, 40 * r.len, 58 * r.len, 1.6, "#ffae3a", 0.85)).join(""),
    () => rays.filter((_, i) => i % 2 === 0).map((r) => {
      const d = 58 * r.len;
      return `<circle cx="${num(MID + Math.cos(r.a) * d)}" cy="${num(MID + Math.sin(r.a) * d + 4)}" r="1.8" fill="#ff8a2a" opacity="0.7"/>`;
    }).join(""),
  ];
  const body = frames.map((f, i) => `<g transform="translate(${i * FX},0)">${f()}</g>`).join("");
  return svg(FX * frames.length, FX, body, defs);
}

/** Вспышка удара: белая звезда, потом кольцо расходится и гаснет. 3 кадра. */
export function impactSvg() {
  const defs = radial("impactCore", [[0, "#ffffff"], [0.5, "#fff4dc"], [1, "#ffd9a0", 0]]);
  const star = (r0, r1, n, turn) => {
    const pts = [];
    for (let i = 0; i < n * 2; i++) {
      const a = turn + (i / (n * 2)) * Math.PI * 2;
      const r = i % 2 === 0 ? r1 : r0;
      pts.push([MID + Math.cos(a) * r, MID + Math.sin(a) * r]);
    }
    return `M${pts.map(([x, y]) => `${num(x)},${num(y)}`).join("L")}Z`;
  };
  const frames = [
    () => core("impactCore", 34) + `<path d="${star(12, 54, 8, 0.2)}" fill="#fffaf0"/>`,
    () => core("impactCore", 20, 0.7) + `<path d="${star(8, 32, 8, 0.6)}" fill="#fff4dc" opacity="0.8"/>` +
      `<circle cx="${MID}" cy="${MID}" r="38" fill="none" stroke="#fff4dc" stroke-width="6" opacity="0.7"/>`,
    () => `<circle cx="${MID}" cy="${MID}" r="52" fill="none" stroke="#ffe9c0" stroke-width="2.5" opacity="0.45"/>`,
  ];
  const body = frames.map((f, i) => `<g transform="translate(${i * FX},0)">${f()}</g>`).join("");
  return svg(FX * frames.length, FX, body, defs);
}
