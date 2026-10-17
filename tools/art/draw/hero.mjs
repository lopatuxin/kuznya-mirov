// Илья по частям: поза — углы суставов, кадр — поза, лист кадров — набор поз.
// Смотрит вправо; ближние к зрителю рука и нога — левые, дальние темнее и позади.
import { svg, capsule, smoothClosed, shade, num } from "./brush.mjs";

/** Кадр 3 × 3 клетки по 96 точек; ступни стоят на нижней кромке. */
const FRAME = 288;
const GROUND = FRAME - 3;
const CX = FRAME / 2;

const C = {
  outline: "#2b1f17",
  shirt: "#efe6d2",
  shirtDark: "#c2b495",
  embroidery: "#b2302b",
  belt: "#bb372d",
  beltDark: "#7a1f18",
  trousers: "#62503d",
  trousersDark: "#3e3227",
  wrap: "#e2d7bd",
  wrapDark: "#b5a786",
  strap: "#6a5034",
  lapti: "#cba25a",
  laptiDark: "#8a6630",
  skin: "#e6ad8d",
  skinDark: "#c07f63",
  hair: "#6e4728",
  hairLight: "#9a6a3c",
  hairDark: "#4a2e19",
  iron: "#a7b0b6",
  ironDark: "#5b646b",
  haft: "#8e5c2f",
  haftDark: "#5e3a1b",
};

/** Длины частей в точках: рост около 186, семь с половиной голов. */
const L = { torso: 58, neck: 5, upperArm: 33, foreArm: 29, thigh: 44, shin: 42, ankle: 8, foot: 31, heel: 8 };

const rad = (d) => (d * Math.PI) / 180;
/** Направление по углу от «прямо вниз», плюс — вперёд (вправо). */
const down = (deg) => [Math.sin(rad(deg)), Math.cos(rad(deg))];
const add = (p, v, k) => [p[0] + v[0] * k, p[1] + v[1] * k];

let gid = 0;

/** Часть тела палкой с переходом цвета поперёк: светлая сторона — к свету слева сверху. */
function limb(a, b, wa, wb, light, dark, far) {
  const id = `g${gid++}`;
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len = Math.hypot(dx, dy) || 1;
  let nx = -dy / len;
  let ny = dx / len;
  if (nx + ny > 0) {
    nx = -nx;
    ny = -ny;
  }
  const m = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  const w = Math.max(wa, wb) / 2;
  const l = far ? shade(light, -0.22) : light;
  const d = far ? shade(dark, -0.22) : dark;
  const def = `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(m[0] + nx * w)}" y1="${num(m[1] + ny * w)}" x2="${num(m[0] - nx * w)}" y2="${num(m[1] - ny * w)}"><stop offset="0" stop-color="${shade(l, 0.12)}"/><stop offset="0.45" stop-color="${l}"/><stop offset="1" stop-color="${d}"/></linearGradient>`;
  return { def, body: `<path d="${capsule(a, b, wa, wb)}" fill="url(#${id})" stroke="${C.outline}" stroke-width="2.2" stroke-linejoin="round"/>` };
}

function poly(points, fill, { stroke = C.outline, width = 2.2, smooth = 0, opacity = 1 } = {}) {
  const d = smooth ? smoothClosed(points, smooth) : `M${points.map(([x, y]) => `${num(x)},${num(y)}`).join("L")}Z`;
  return `<path d="${d}" fill="${fill}" stroke="${stroke}" stroke-width="${width}" stroke-linejoin="round" opacity="${opacity}"/>`;
}

/** Нога: бедро в портах, голень в онучах с оборами, лапоть. */
function leg(hip, p, far) {
  const knee = add(hip, down(p.thigh), L.thigh);
  const shinDir = down(p.thigh - p.knee);
  const ankle = add(knee, shinDir, L.shin);
  // Ступня: от пятки к носку под углом `foot` от горизонтали (плюс — носок вниз).
  const fd = [Math.cos(rad(p.foot)), Math.sin(rad(p.foot))];
  const sole = add(ankle, down(p.thigh - p.knee), L.ankle);
  const heel = add(sole, fd, -L.heel);
  const toe = add(sole, fd, L.foot - L.heel);
  let defs = "";
  let body = "";
  const shin = limb(knee, ankle, 22, 16, C.wrap, C.wrapDark, far);
  const thigh = limb(hip, knee, 33, 25, C.trousers, C.trousersDark, far);
  defs += shin.def + thigh.def;
  body += shin.body;
  // Оборы крест-накрест по онучам.
  const strapC = far ? shade(C.strap, -0.2) : C.strap;
  for (let k = 0.18; k < 0.95; k += 0.2) {
    const pa = add(knee, shinDir, L.shin * k);
    const pb = add(knee, shinDir, L.shin * (k + 0.12));
    const n = [-shinDir[1], shinDir[0]];
    body += `<path d="M${num(pa[0] + n[0] * 7.5)},${num(pa[1] + n[1] * 7.5)}L${num(pb[0] - n[0] * 7)},${num(pb[1] - n[1] * 7)}" stroke="${strapC}" stroke-width="2.4" stroke-linecap="round"/>`;
  }
  // Лапоть: округлый, носок чуть вздёрнут, плетение полосками.
  const up = [fd[1], -fd[0]];
  const lap = [
    add(heel, up, 2),
    add(add(heel, fd, -2), up, 9),
    add(add(sole, fd, 2), up, 14),
    add(add(toe, fd, -6), up, 10),
    add(toe, up, 4),
    add(add(toe, fd, -3), up, -1),
    add(add(sole, fd, 4), up, -2),
    add(heel, up, -2),
  ];
  const lc = far ? shade(C.lapti, -0.22) : C.lapti;
  body += poly(lap, lc, { smooth: 0.9 });
  for (let k = 0.15; k < 0.95; k += 0.17) {
    const p0 = add(add(heel, fd, L.foot * k), up, 0);
    const p1 = add(add(heel, fd, L.foot * k + 3), up, 10);
    body += `<path d="M${num(p0[0])},${num(p0[1])}L${num(p1[0])},${num(p1[1])}" stroke="${far ? shade(C.laptiDark, -0.2) : C.laptiDark}" stroke-width="1.6"/>`;
  }
  body += thigh.body;
  return { defs, body, lowest: Math.max(heel[1] + 2, toe[1] + 4, sole[1] + 2), knee, ankle };
}

/** Рука: рукав рубахи с вышитым обшлагом, кулак; в ближней — топор. */
function arm(shoulder, p, far, axe) {
  const elbow = add(shoulder, down(p.upper), L.upperArm);
  const foreDir = down(p.upper + p.elbow);
  const wrist = add(elbow, foreDir, L.foreArm);
  const hand = add(wrist, foreDir, 6);
  let defs = "";
  let body = "";
  let axeBody = "";
  if (axe) {
    // Топорище поперёк кулака: от `axeAngle` относительно предплечья.
    const hd = down(p.upper + p.elbow + p.axeAngle);
    const butt = add(hand, hd, -16);
    const top = add(hand, hd, 44);
    const haft = limb(butt, top, 6.5, 5.5, C.haft, C.haftDark, false);
    defs += haft.def;
    const side = [hd[1], -hd[0]]; // поперёк топорища, вперёд — лезвие
    const s = p.axeFlip ? -1 : 1;
    const b0 = add(top, hd, -14);
    const blade = [
      add(add(top, hd, 2), side, -s * 4),
      add(add(top, hd, 6), side, s * 9),
      add(add(top, hd, 12), side, s * 20),
      add(add(top, hd, -4), side, s * 23),
      add(add(top, hd, -20), side, s * 19),
      add(add(b0, hd, -2), side, s * 8),
      add(b0, side, -s * 4),
    ];
    axeBody += haft.body + poly(blade, C.iron, { smooth: 0.35 });
    const e0 = add(add(top, hd, 11), side, s * 19);
    const e1 = add(add(top, hd, -19), side, s * 18);
    axeBody += `<path d="M${num(e0[0])},${num(e0[1])}L${num(e1[0])},${num(e1[1])}" stroke="#e7eef2" stroke-width="2.4" stroke-linecap="round"/>`;
  }
  const upper = limb(shoulder, elbow, 22, 18, C.shirt, C.shirtDark, far);
  const fore = limb(elbow, wrist, 18, 15, C.shirt, C.shirtDark, far);
  defs += upper.def + fore.def;
  body += upper.body + fore.body;
  // Обшлаг с вышивкой.
  const c0 = add(wrist, foreDir, -5);
  const n = [-foreDir[1], foreDir[0]];
  body += `<path d="M${num(c0[0] + n[0] * 6.5)},${num(c0[1] + n[1] * 6.5)}L${num(c0[0] - n[0] * 6.5)},${num(c0[1] - n[1] * 6.5)}" stroke="${far ? shade(C.embroidery, -0.25) : C.embroidery}" stroke-width="3.5"/>`;
  const fist = `<circle cx="${num(hand[0])}" cy="${num(hand[1])}" r="8.5" fill="${far ? shade(C.skin, -0.2) : C.skin}" stroke="${C.outline}" stroke-width="2.2"/>`;
  return { defs, body: body + (axe ? fist + axeBody : fist), hand };
}

/** Туловище: рубаха до середины бедра, кушак с кистями, вышивка у ворота и подола. */
function torso(hip, p) {
  const up = [Math.sin(rad(p.lean)), -Math.cos(rad(p.lean))];
  const fwd = [-up[1], up[0]];
  const at = (u, v) => [hip[0] + up[0] * u + fwd[0] * v, hip[1] + up[1] * u + fwd[1] * v];
  const breath = p.breath ?? 0;
  const neck = at(L.torso + breath, 6);
  const shoulder = at(L.torso - 10 + breath, -2);
  const shirt = [
    at(-22, -24), at(-6, -22), at(14, -23), at(34, -25), at(50 + breath, -22), at(58 + breath, -13),
    at(61 + breath, 2), at(58 + breath, 17), at(46 + breath, 27), at(30, 28), at(12, 26), at(-6, 26), at(-23, 27),
  ];
  let defs = `<linearGradient id="shirt${gid}" gradientUnits="userSpaceOnUse" x1="${num(at(40, -20)[0])}" y1="${num(at(40, -20)[1])}" x2="${num(at(10, 22)[0])}" y2="${num(at(10, 22)[1])}"><stop offset="0" stop-color="${C.shirtDark}"/><stop offset="0.5" stop-color="${C.shirt}"/><stop offset="1" stop-color="${shade(C.shirt, -0.06)}"/></linearGradient>`;
  let body = poly(shirt, `url(#shirt${gid})`, { smooth: 0.6 });
  gid++;
  // Складки подола и вышивка по подолу.
  body += `<path d="M${num(at(-21, -16)[0])},${num(at(-21, -16)[1])}Q${num(at(-23, 3)[0])},${num(at(-23, 3)[1])} ${num(at(-22, 20)[0])},${num(at(-22, 20)[1])}" stroke="${C.embroidery}" stroke-width="4" fill="none"/>`;
  body += `<path d="M${num(at(-4, 6)[0])},${num(at(-4, 6)[1])}L${num(at(-19, 4)[0])},${num(at(-19, 4)[1])}" stroke="${C.shirtDark}" stroke-width="2" stroke-linecap="round"/>`;
  // Ворот с вышивкой.
  body += `<path d="M${num(at(57 + breath, 5)[0])},${num(at(57 + breath, 5)[1])}Q${num(at(52 + breath, 14)[0])},${num(at(52 + breath, 14)[1])} ${num(at(44 + breath, 19)[0])},${num(at(44 + breath, 19)[1])}" stroke="${C.embroidery}" stroke-width="4" fill="none" stroke-linecap="round"/>`;
  // Кушак и его концы, отстающие при беге.
  const belt = [at(4, -23), at(4, 25), at(-5, 26), at(-5, -23)];
  body += poly(belt, C.belt, { width: 2 });
  const knot = at(0, 18);
  const sw = p.sash ?? 0;
  for (const [dl, len] of [[0, 30], [5, 24]]) {
    const end = [knot[0] - 8 - sw * 0.6 + dl, knot[1] + len - Math.abs(sw) * 0.3];
    body += `<path d="M${num(knot[0] - 3 + dl * 0.3)},${num(knot[1])}Q${num(knot[0] - 4 - sw * 0.3 + dl)},${num(knot[1] + len * 0.5)} ${num(end[0])},${num(end[1])}" stroke="${C.beltDark}" stroke-width="5" fill="none" stroke-linecap="round"/>`;
    body += `<path d="M${num(end[0] - 3)},${num(end[1])}L${num(end[0] + 2)},${num(end[1] + 7)}M${num(end[0])},${num(end[1])}L${num(end[0] + 4)},${num(end[1] + 5)}" stroke="${C.belt}" stroke-width="2"/>`;
  }
  return { defs, body, neck, shoulder, up, fwd };
}

/** Голова в профиль: волосы, борода, усы, нос, глаз, бровь. */
function head(neck, p, up, fwd) {
  const tilt = p.headTilt ?? 0;
  const hu = [Math.sin(rad(p.lean + tilt)), -Math.cos(rad(p.lean + tilt))];
  const hf = [-hu[1], hu[0]];
  const c = add(add(neck, hu, 17), hf, 2);
  const at = (u, v) => [c[0] + hu[0] * u + hf[0] * v, c[1] + hu[1] * u + hf[1] * v];
  let body = "";
  // Шея.
  body += poly([at(-20, -6), at(-6, -7), at(-6, 6), at(-20, 7)], C.skinDark, { width: 2 });
  // Лицо и череп.
  const face = [at(12, -11), at(15, 0), at(12, 9), at(6, 12), at(3, 13.5), at(0, 15.5), at(-2, 13), at(-6, 13), at(-12, 9), at(-14, 0), at(-10, -11), at(0, -14)];
  body += poly(face, C.skin, { smooth: 0.7 });
  // Борода окладистая: от уха до груди.
  const beard = [at(-1, -4), at(-3, 9), at(-6, 15), at(-13, 15), at(-20, 12), at(-24, 6), at(-23, -2), at(-17, -7), at(-8, -7)];
  body += poly(beard, C.hair, { smooth: 0.75 });
  body += `<path d="M${num(at(-10, 8)[0])},${num(at(-10, 8)[1])}Q${num(at(-17, 6)[0])},${num(at(-17, 6)[1])} ${num(at(-22, 4)[0])},${num(at(-22, 4)[1])}M${num(at(-8, 1)[0])},${num(at(-8, 1)[1])}Q${num(at(-15, -1)[0])},${num(at(-15, -1)[1])} ${num(at(-20, -2)[0])},${num(at(-20, -2)[1])}" stroke="${C.hairLight}" stroke-width="1.8" fill="none" stroke-linecap="round"/>`;
  // Усы.
  body += poly([at(-1, 7), at(-2, 14), at(-5, 15), at(-7, 10), at(-4, 5)], C.hairDark, { smooth: 0.8, width: 1.6 });
  // Нос.
  body += `<path d="M${num(at(4, 13)[0])},${num(at(4, 13)[1])}Q${num(at(1, 18.5)[0])},${num(at(1, 18.5)[1])} ${num(at(-1, 14)[0])},${num(at(-1, 14)[1])}" stroke="${C.outline}" stroke-width="1.6" fill="${C.skin}"/>`;
  // Волосы: шапка до затылка, прядь у виска.
  const hair = [at(-3, -15), at(6, -16), at(13, -12), at(17, -2), at(16, 8), at(12, 12), at(10, 9), at(11, 3), at(7, -2), at(3, -5), at(1, -9), at(-4, -12), at(-5, -16)];
  body += poly(hair, C.hair, { smooth: 0.7 });
  body += `<path d="M${num(at(12, -9)[0])},${num(at(12, -9)[1])}Q${num(at(8, -3)[0])},${num(at(8, -3)[1])} ${num(at(-6, -12)[0])},${num(at(-6, -12)[1])}" stroke="${C.hairLight}" stroke-width="2" fill="none" stroke-linecap="round"/>`;
  // Ухо, глаз, бровь.
  body += `<ellipse cx="${num(at(0, -5)[0])}" cy="${num(at(0, -5)[1])}" rx="3.2" ry="4.6" fill="${C.skinDark}" stroke="${C.outline}" stroke-width="1.6"/>`;
  body += `<ellipse cx="${num(at(5, 9)[0])}" cy="${num(at(5, 9)[1])}" rx="1.7" ry="2.1" fill="${C.outline}"/>`;
  body += `<path d="M${num(at(8.5, 6)[0])},${num(at(8.5, 6)[1])}L${num(at(8.5, 12)[0])},${num(at(8.5, 12)[1])}" stroke="${C.hairDark}" stroke-width="2.8" stroke-linecap="round"/>`;
  return body;
}

/** Рисует одну позу; ступни у земли, середина таза — посередине кадра. */
function pose(p) {
  const hip0 = [CX + (p.dx ?? 0), 100];
  const legNear0 = leg(hip0, p.near, false);
  const legFar0 = leg(hip0, p.far, true);
  const lowest = Math.max(legNear0.lowest, legFar0.lowest);
  const hip = [hip0[0], hip0[1] + (GROUND - lowest) - (p.lift ?? 0)];
  const legNear = leg(hip, p.near, false);
  const legFar = leg(hip, p.far, true);
  const t = torso(hip, p);
  const armFar = arm(t.shoulder, p.farArm, true, false);
  const armNear = arm(t.shoulder, p.nearArm, false, true);
  const h = head(t.neck, p, t.up, t.fwd);
  const defs = legFar.defs + legNear.defs + t.defs + armFar.defs + armNear.defs;
  const body = armFar.body + legFar.body + legNear.body + t.body + h + armNear.body;
  return { defs, body };
}

const base = { lean: 3, breath: 0, sash: 0, headTilt: 0 };
const nearArmIdle = { upper: 10, elbow: 25, axeAngle: -80, axeFlip: false };

/** Стойка: 4 кадра дыхания. */
const IDLE = [0, 1, 2, 1].map((b) => ({
  ...base,
  breath: b * 0.8,
  near: { thigh: 5, knee: 4, foot: 0 },
  far: { thigh: -6, knee: 4, foot: 0 },
  nearArm: { ...nearArmIdle, upper: 10 - b, elbow: 28 + b * 2 },
  farArm: { upper: -8 + b, elbow: 14 },
}));

/** Ход ноги за круг бега: бедро, колено, ступня по 8 кадрам. */
const RUN_LEG = [
  { thigh: 32, knee: 12, foot: -12 },
  { thigh: 16, knee: 38, foot: 0 },
  { thigh: -6, knee: 28, foot: 0 },
  { thigh: -28, knee: 18, foot: 12 },
  { thigh: -34, knee: 50, foot: 28 },
  { thigh: -12, knee: 92, foot: 38 },
  { thigh: 22, knee: 98, foot: 22 },
  { thigh: 44, knee: 58, foot: -4 },
];
const RUN_LIFT = [0, 0, 2, 6, 0, 0, 2, 6];

/** Бег: 8 кадров; ближняя рука ходит навстречу ближней ноге. */
const RUN = RUN_LEG.map((nearLeg, i) => {
  const farLeg = RUN_LEG[(i + 4) % 8];
  return {
    ...base,
    lean: 11,
    headTilt: -6,
    lift: RUN_LIFT[i],
    sash: -10,
    near: nearLeg,
    far: farLeg,
    nearArm: { upper: -0.9 * (nearLeg.thigh - 4), elbow: 78, axeAngle: 95, axeFlip: true },
    farArm: { upper: -0.9 * (farLeg.thigh - 4), elbow: 82 },
  };
});

/** Прыжок: подъём, верх, падение. */
const JUMP = [
  { ...base, lean: 10, headTilt: -8, sash: -14, near: { thigh: 58, knee: 30, foot: -10 }, far: { thigh: -32, knee: 88, foot: 35 },
    nearArm: { upper: -35, elbow: 75, axeAngle: 95, axeFlip: true }, farArm: { upper: 70, elbow: 45 } },
  { ...base, lean: 6, headTilt: -4, sash: -6, near: { thigh: 68, knee: 100, foot: 5 }, far: { thigh: 25, knee: 112, foot: 25 },
    nearArm: { upper: 15, elbow: 85, axeAngle: 90, axeFlip: true }, farArm: { upper: 45, elbow: 70 } },
  { ...base, lean: 2, headTilt: 2, sash: 8, near: { thigh: 26, knee: 38, foot: 8 }, far: { thigh: -8, knee: 30, foot: 12 },
    nearArm: { upper: 50, elbow: 40, axeAngle: 95, axeFlip: true }, farArm: { upper: -45, elbow: 30 } },
];

/** Лист кадров: стойка 0–3, бег 4–11, прыжок 12–14; по 5 в строке. */
const SHEET = [...IDLE, ...RUN, ...JUMP];
const COLUMNS = 5;

export function sheetSvg() {
  const rows = Math.ceil(SHEET.length / COLUMNS);
  let defs = "";
  let body = "";
  SHEET.forEach((p, i) => {
    const { defs: d, body: b } = pose(p);
    defs += d;
    const x = (i % COLUMNS) * FRAME;
    const y = Math.floor(i / COLUMNS) * FRAME;
    body += `<g transform="translate(${x},${y})">${b}</g>`;
  });
  return svg(COLUMNS * FRAME, rows * FRAME, body, defs);
}

