// Алёша Попович на общем скелете человека: голубая рубаха с красной вышивкой, жёлтый кушак с кистями,
// бурые порты, онучи, лапти, кинжал. Начальная одежда — как на «Распутье» (art/platformer/start/alyosha.png).
// Позы и раскладка листа — Ильи.
import { shade, num } from "./brush.mjs";
import { add, down, uid, limb, poly, curve, line, fist, face, nose, earAndEye, sheet } from "./figure.mjs";
import { SHEET, COLUMNS, FRAME } from "./hero.mjs";

const C = {
  shirt: "#79bde8",
  shirtDark: "#4a87b8",
  sleeve: "#79bde8",
  sleeveDark: "#4a87b8",
  embroidery: "#c43448",
  belt: "#e2b43c",
  beltDark: "#a7791d",
  trousers: "#86552f",
  trousersDark: "#57351b",
  wrap: "#efe6d6",
  wrapDark: "#bdb09a",
  strap: "#4b3a2e",
  lapti: "#b5683a",
  laptiDark: "#7a4020",
  skin: "#eab593",
  skinDark: "#c4866a",
  hair: "#d7a24c",
  hairLight: "#f2cb7a",
  hairDark: "#a26f2a",
  steel: "#c9d1d6",
  steelDark: "#6d777e",
  brass: "#c9973a",
  grip: "#6a3f22",
};

/** Кинжал в кулаке: клинок от кулака туда, куда показывает `axeAngle` от предплечья, за кулаком — навершие. */
function dagger(hand, foreDir, p) {
  const hd = down(p.upper + p.elbow + p.axeAngle);
  const side = [hd[1], -hd[0]];
  const grip = limb(add(hand, hd, -13), add(hand, hd, 9), 6, 5.5, C.grip, shade(C.grip, -0.3), false);
  const pommel = add(hand, hd, -14);
  let behind = grip.body + `<circle cx="${num(pommel[0])}" cy="${num(pommel[1])}" r="4" fill="${C.brass}" stroke="#2b1f17" stroke-width="1.8"/>`;
  const g0 = add(hand, hd, 10);
  const blade = [add(g0, side, -4.5), add(add(g0, hd, 26), side, -2.5), add(g0, hd, 36), add(add(g0, hd, 26), side, 2.5), add(g0, side, 4.5)];
  behind += poly(blade, C.steel, { width: 1.8 });
  behind += line(add(g0, hd, 2), add(g0, hd, 30), C.steelDark, 1.2);
  behind += line(add(g0, side, -8), add(g0, side, 8), C.brass, 5);
  return { def: grip.def, behind, front: fist(hand, hd, foreDir, C) };
}

/** Обшлаг рукава с вышивкой. */
function cuff(wrist, foreDir, far) {
  const c0 = add(wrist, foreDir, -5);
  const n = [-foreDir[1], foreDir[0]];
  return `<path d="M${num(c0[0] + n[0] * 6.5)},${num(c0[1] + n[1] * 6.5)}L${num(c0[0] - n[0] * 6.5)},${num(c0[1] - n[1] * 6.5)}" stroke="${far ? shade(C.embroidery, -0.25) : C.embroidery}" stroke-width="4.5"/>`;
}

/** Рубаха до середины бедра, поуже, чем у Ильи; вышивка у ворота и по подолу, кушак с длинными кистями. */
function garment(at, p) {
  const breath = p.breath ?? 0;
  const shirt = [
    at(-21, -21), at(-6, -19), at(14, -20), at(34, -22), at(50 + breath, -19), at(57 + breath, -11),
    at(59 + breath, 2), at(56 + breath, 15), at(46 + breath, 23), at(30, 23), at(12, 22), at(-6, 23), at(-22, 24),
  ];
  const id = uid("shirt");
  const defs = `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(at(40, -20)[0])}" y1="${num(at(40, -20)[1])}" x2="${num(at(10, 22)[0])}" y2="${num(at(10, 22)[1])}"><stop offset="0" stop-color="${C.shirtDark}"/><stop offset="0.5" stop-color="${C.shirt}"/><stop offset="1" stop-color="${shade(C.shirt, -0.06)}"/></linearGradient>`;
  let body = poly(shirt, `url(#${id})`, { smooth: 0.6 });
  // Вышивка по подолу: красная полоса с белыми зубцами.
  body += `<path d="M${num(at(-18, -20)[0])},${num(at(-18, -20)[1])}L${num(at(-19, 23)[0])},${num(at(-19, 23)[1])}" stroke="${C.embroidery}" stroke-width="5"/>`;
  for (let v = -16; v <= 20; v += 6) body += line(at(-17, v), at(-20, v + 3), "#f4ece0", 1.3);
  // Ворот с вышивкой до груди.
  body += curve(at(56 + breath, 5), at(50 + breath, 13), at(42 + breath, 17), C.embroidery, 4.5);
  body += line(at(56 + breath, 9), at(46 + breath, 10), C.embroidery, 3);
  // Кушак и кисти, отстающие при беге.
  const belt = [at(5, -21), at(5, 23), at(-5, 24), at(-5, -21)];
  body += poly(belt, C.belt, { width: 2 });
  body += line(at(0, -18), at(0, 20), C.beltDark, 1.4);
  const knot = at(0, 14);
  const sw = p.sash ?? 0;
  for (const [dl, len] of [[0, 36], [6, 30]]) {
    const end = [knot[0] - 6 - sw * 0.6 + dl, knot[1] + len - Math.abs(sw) * 0.3];
    body += `<path d="M${num(knot[0] - 2 + dl * 0.3)},${num(knot[1])}Q${num(knot[0] - 3 - sw * 0.3 + dl)},${num(knot[1] + len * 0.5)} ${num(end[0])},${num(end[1])}" stroke="${C.beltDark}" stroke-width="5" fill="none" stroke-linecap="round"/>`;
    body += poly([[end[0] - 4, end[1] - 2], [end[0] + 4, end[1] - 2], [end[0] + 6, end[1] + 10], [end[0] - 5, end[1] + 10]], C.belt, { width: 1.6 });
  }
  return { defs, body };
}

/** Голова: русые волнистые волосы до шеи, прядь на лоб, короткая бородка. */
function head(at) {
  let body = face(at, C);
  // Бородка по челюсти и под губой.
  body += poly([at(-3, 6), at(-8, 12), at(-14, 9), at(-15, 0), at(-11, -6), at(-8, -2), at(-9, 6)], C.hairDark, { smooth: 0.7, width: 1.4, opacity: 0.85 });
  body += nose(at, C);
  // Волосы: шапка с волной на затылке до шеи, прядь на лоб.
  const hair = [
    at(-2, -15), at(8, -17), at(15, -12), at(19, -3), at(17, 7), at(13, 12), at(11, 7), at(12, 2), at(8, -1),
    at(3, -6), at(-4, -9), at(-10, -10), at(-17, -12), at(-20, -17), at(-14, -19), at(-8, -18),
  ];
  body += poly(hair, C.hair, { smooth: 0.7 });
  body += curve(at(16, -8), at(10, -4), at(-2, -14), C.hairLight, 2.2);
  body += curve(at(6, -11), at(-6, -13), at(-15, -15), C.hairLight, 1.8);
  body += curve(at(17, 4), at(13, 9), at(9, 10), C.hairDark, 1.6);
  body += earAndEye(at, C);
  body += line(at(8.5, 6), at(8, 12), C.hairDark, 2.4);
  return body;
}

const ALYOSHA = { C, garment, head, cuff, weapon: dagger };

/** След взмаха — по острию кинжала: он короче топора, след ближе к плечу и уже. */
const POSES = SHEET.map((p) => (p.trail ? { ...p, trail: { ...p.trail, r: 110, width: p.trail.width * 0.75 } } : p));

export function alyoshaSvg() {
  return sheet(POSES, ALYOSHA, FRAME, COLUMNS);
}
