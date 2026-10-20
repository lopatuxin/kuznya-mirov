// Картинки первого уровня платформера: что нарисовать и в каком размере.
// Числа в клетках сцены; `y` растёт вниз, низ сцены — 12. Уступы нарисованы под контуры земли
// в `games/platformer/scene.json`: `top` — верх контура, обрыв `left`/`right` стоит на краю контура,
// а край без обрыва заходит под соседний уступ, поэтому `x0`–`x1` бывают шире контура.
import { ledge } from "./ledge.mjs";
import { CELL } from "./brush.mjs";
import { bigSpruce, pine, weepingBirch } from "./trees.mjs";
import { sky, cloud, mountains, forest, kremlin } from "./background.mjs";
import { gorge, hangingLog, fallenLog, boulder, mushrooms, gate, foreground } from "./props.mjs";
import { sheetSvg } from "./hero.mjs";
import { robberSvg, robberFlashSvg, robberAuraSvg } from "./robber.mjs";
import { clashSvg, impactSvg } from "./fx.mjs";

const BOTTOM = 12.6;
const LEDGE_TOP = 0.45;
const PAD = 0.35;

/** Уступы: `x0`–`x1` — где нарисован верх, `top` — где стоят, `from` — левый край картинки, если он не по `x0`. */
const LEDGES = [
  { name: "ledge_a", x0: 0, x1: 23.0, top: 9.0, left: false, right: false, from: -0.5, seed: 11, split: true },
  { name: "ledge_b", x0: 22, x1: 34, top: 7.8, left: true, right: true, seed: 13 },
  { name: "ledge_c", x0: 37, x1: 50, top: 8.5, left: true, right: true, seed: 14 },
  { name: "ledge_d", x0: 64, x1: 88, top: 8.0, left: true, right: false, seed: 15, split: true },
  { name: "ledge_f", x0: 86, x1: 100, top: 5.6, left: true, right: true, seed: 17 },
  { name: "ledge_g", x0: 98, x1: 112, top: 8.8, left: false, right: true, seed: 18 },
  { name: "ledge_h", x0: 116.5, x1: 150.5, top: 8.2, left: true, right: false, seed: 19, split: true },
];

/** Картинки: имя → как нарисовать и сколько точек на клетку (дальнее — мельче, всё равно в дымке). */
export const IMAGES = {
  sky: { draw: () => sky(), cells: [8, 12], ppc: 24 },
  cloud_a: { draw: () => cloud(1, 5, 2), cells: [5, 2] },
  cloud_b: { draw: () => cloud(2, 3.5, 1.5), cells: [3.5, 1.5] },
  cloud_c: { draw: () => cloud(3, 7, 2.4), cells: [7, 2.4] },
  mountains: { draw: () => mountains(), cells: [32, 6], ppc: 48 },
  kremlin: { draw: () => kremlin(), cells: [10, 5] },
  forest_far: { draw: () => forest({ w: 24, h: 5, seed: 21, far: 0.75, count: 120, minH: 1.2, maxH: 2.6, birches: 0.3 }), cells: [24, 5], ppc: 64 },
  forest_near: { draw: () => forest({ w: 20, h: 7, seed: 22, far: 0.35, count: 55, minH: 2.5, maxH: 5, birches: 0.3, groundFrom: 0.85 }), cells: [20, 7] },
  pine: { draw: () => pine({ seed: 31 }), cells: [3.6, 10.5] },
  spruce: { draw: () => bigSpruce({ seed: 33 }), cells: [4.4, 10.5] },
  birch: { draw: () => weepingBirch({ seed: 32 }), cells: [3.4, 10.5] },
  gorge_a: { draw: () => gorge({ w: 5, seed: 41 }), cells: [5, 3.2] },
  gorge_b: { draw: () => gorge({ w: 16, seed: 42 }), cells: [16, 3.2] },
  gorge_c: { draw: () => gorge({ w: 6.5, seed: 43 }), cells: [6.5, 3.2] },
  hanging_log: { draw: () => hangingLog({ w: 4.2, h: 7.0, seed: 51 }), cells: [4.2, 7.0] },
  fallen_log: { draw: () => fallenLog({ w: 5, h: 1.8, seed: 52 }), cells: [5, 1.8] },
  boulder: { draw: () => boulder({ w: 4.2, h: 2.4, seed: 53 }), cells: [4.2, 2.4] },
  stone: { draw: () => boulder({ w: 2.4, h: 1.3, seed: 54 }), cells: [2.4, 1.3] },
  mushrooms: { draw: () => mushrooms({ seed: 61 }), cells: [1.4, 1.0] },
  mushrooms_glow: { draw: () => mushrooms({ seed: 61, glow: true }), cells: [1.4, 1.0] },
  gate: { draw: () => gate(), cells: [4.6, 5.4] },
  foreground: { draw: () => foreground({ w: 30, h: 2.4, seed: 71 }), cells: [30, 2.4], ppc: 48 },
  hero: { draw: () => ({ svg: sheetSvg() }) },
  robber: { draw: () => ({ svg: robberSvg() }) },
  robber_flash: { draw: () => ({ svg: robberFlashSvg() }) },
  robber_aura: { draw: () => ({ svg: robberAuraSvg() }) },
  clash: { draw: () => ({ svg: clashSvg() }) },
  impact: { draw: () => ({ svg: impactSvg() }) },
};
for (const l of LEDGES) {
  const from = l.from ?? l.x0 - (l.left ? PAD : 0);
  const to = l.x1 + (l.right ? PAD : 0);
  const w = to - from;
  const h = BOTTOM - l.top + LEDGE_TOP;
  const draw = () => ledge({ w, h, top: LEDGE_TOP, pad: PAD, left: l.left, right: l.right, seed: l.seed });
  let parts = [{ name: l.name, px: null }];
  if (l.split) {
    // Шире атласа: рисуется целиком и режется пополам, куски заходят друг на друга на 2 точки — шва нет.
    const total = Math.round(w * CELL);
    const half = Math.round(total / 2);
    parts = [
      { name: `${l.name}1`, px: [0, half + 2] },
      { name: `${l.name}2`, px: [half, total - half] },
    ];
  }
  for (const p of parts) {
    IMAGES[p.name] = { draw, cells: [p.px ? p.px[1] / CELL : w, h], crop: p.px };
  }
}
