// Разбойник на общем скелете человека: меховая шапка, рыжий рваный кафтан с заплатами, верёвочный пояс,
// тёмные порты, онучи, лапти, чёрная борода, дубина с сучьями. Поза — углы суставов, как у Ильи.
// Тот же лист кадров даёт и отсветы: белый силуэт — вспышка от удара, красный ореол — особый замах.
import { shade, num } from "./brush.mjs";
import { add, down, uid, limb, poly, curve, line, fist, face, nose, earAndEye, sheet } from "./figure.mjs";

const C = {
  kaftan: "#a4482a",
  kaftanDark: "#6b2a17",
  patch: "#6e4b2e",
  undershirt: "#d9cfb6",
  sleeve: "#a4482a",
  sleeveDark: "#6b2a17",
  rope: "#c9b07a",
  ropeDark: "#8a7346",
  trousers: "#3f3a35",
  trousersDark: "#27231f",
  wrap: "#d3c6a6",
  wrapDark: "#a39674",
  strap: "#5a4430",
  lapti: "#b8924f",
  laptiDark: "#7a5a2a",
  skin: "#d99a78",
  skinDark: "#a86a50",
  beard: "#2a1f1a",
  beardLight: "#4a3a30",
  hat: "#5a4636",
  hatDark: "#3a2c22",
  hatFur: "#7d6650",
  wood: "#7a5232",
  woodDark: "#4c3019",
  knot: "#3e2614",
};

/** Дубина: тонкая рукоять у кулака, толстеет к концу; на ней сучки и пара вбитых гвоздей. */
const CLUB = 82;
const CLUB_GRIP = 11;

function club(hand, foreDir, p) {
  const hd = down(p.upper + p.elbow + p.clubAngle);
  const butt = add(hand, hd, -CLUB_GRIP);
  const mid = add(hand, hd, 26);
  const tip = add(hand, hd, CLUB - CLUB_GRIP);
  const grip = limb(butt, mid, 8, 10, C.wood, C.woodDark, false);
  const head = limb(mid, tip, 10, 21, C.wood, C.woodDark, false);
  const side = [hd[1], -hd[0]];
  let behind = grip.body + head.body;
  for (const [k, v, r] of [[0.45, 3, 2.6], [0.62, -4, 3.2], [0.8, 5, 3], [0.93, -3, 2.6]]) {
    const c = add(add(hand, hd, (CLUB - CLUB_GRIP) * k), side, v);
    behind += `<ellipse cx="${num(c[0])}" cy="${num(c[1])}" rx="${r}" ry="${r * 0.7}" fill="${C.knot}"/>`;
  }
  for (const [k, s] of [[0.7, 1], [0.86, -1]]) {
    const r = 5 + 5.5 * k;
    const a = add(add(hand, hd, (CLUB - CLUB_GRIP) * k), side, s * r);
    const b = add(a, side, s * 6);
    behind += line(a, b, "#9aa1a6", 2.6);
  }
  return { def: grip.def + head.def, behind, front: fist(hand, hd, foreDir, C) };
}

/** Рваный край рукава у запястья. */
function cuff(wrist, foreDir, far) {
  const c0 = add(wrist, foreDir, -4);
  const n = [-foreDir[1], foreDir[0]];
  const pts = [add(c0, n, 9), add(add(c0, n, 4), foreDir, 5), add(c0, n, 0), add(add(c0, n, -4), foreDir, 4), add(c0, n, -9)];
  return `<path d="M${pts.map(([x, y]) => `${num(x)},${num(y)}`).join("L")}" stroke="${far ? shade(C.kaftanDark, -0.25) : C.kaftanDark}" stroke-width="2.6" fill="none" stroke-linejoin="round"/>`;
}

/** Кафтан выше колена с рваным подолом, распахнут у ворота; заплаты, верёвочный пояс с концами. */
function garment(at, p) {
  const breath = p.breath ?? 0;
  const hem = [at(-38, -26), at(-33, -20), at(-40, -12), at(-34, -4), at(-41, 5), at(-35, 13), at(-40, 22), at(-33, 29)];
  const kaftan = [
    ...hem,
    at(-6, 29), at(14, 29), at(30, 30), at(46 + breath, 29), at(58 + breath, 18), at(61 + breath, 2),
    at(58 + breath, -13), at(50 + breath, -23), at(34, -27), at(14, -26), at(-6, -27),
  ];
  const id = uid("kaftan");
  const defs = `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(at(40, -24)[0])}" y1="${num(at(40, -24)[1])}" x2="${num(at(6, 26)[0])}" y2="${num(at(6, 26)[1])}"><stop offset="0" stop-color="${C.kaftanDark}"/><stop offset="0.5" stop-color="${C.kaftan}"/><stop offset="1" stop-color="${shade(C.kaftan, -0.08)}"/></linearGradient>`;
  let body = poly(kaftan, `url(#${id})`, { smooth: 0.35 });
  // Распахнутый ворот: рубаха под кафтаном.
  body += poly([at(60 + breath, 6), at(56 + breath, 19), at(40 + breath, 26), at(44 + breath, 12)], C.undershirt, { width: 1.8 });
  // Заплаты: на плече и на подоле.
  body += poly([at(44, -18), at(46, -6), at(36, -4), at(34, -16)], C.patch, { width: 1.6 });
  body += poly([at(-22, 10), at(-21, 22), at(-31, 23), at(-32, 11)], C.patch, { width: 1.6 });
  body += line(at(-13, -24), at(-30, -22), C.kaftanDark, 2);
  // Верёвочный пояс, узел спереди и висящие концы.
  body += poly([at(5, -27), at(5, 29), at(-2, 30), at(-2, -27)], C.rope, { width: 1.8 });
  for (let v = -24; v < 28; v += 6) body += line(at(4, v), at(-1, v + 3), C.ropeDark, 1.4);
  const knot = at(1, 22);
  const sw = p.sash ?? 0;
  for (const [dl, len] of [[0, 26], [4, 20]]) {
    const end = [knot[0] - 6 - sw * 0.5 + dl, knot[1] + len - Math.abs(sw) * 0.3];
    body += `<path d="M${num(knot[0])},${num(knot[1])}Q${num(knot[0] - 3 - sw * 0.3 + dl)},${num(knot[1] + len * 0.5)} ${num(end[0])},${num(end[1])}" stroke="${C.ropeDark}" stroke-width="3.4" fill="none" stroke-linecap="round"/>`;
  }
  body += `<circle cx="${num(knot[0])}" cy="${num(knot[1])}" r="4" fill="${C.rope}" stroke="${C.ropeDark}" stroke-width="1.4"/>`;
  return { defs, body };
}

/** Голова: густая чёрная борода, хмурая бровь, патлы из-под меховой шапки. */
function head(at) {
  let body = face(at, C);
  // Патлы из-под шапки на затылке.
  body += poly([at(4, -16), at(-2, -20), at(-14, -19), at(-20, -14), at(-6, -9), at(2, -8)], C.beard, { smooth: 0.6, width: 1.6 });
  // Борода густая, торчит клоками.
  const beard = [at(-1, -5), at(-3, 10), at(-7, 16), at(-15, 16), at(-21, 14), at(-26, 8), at(-24, 2), at(-27, -3), at(-20, -7), at(-9, -8)];
  body += poly(beard, C.beard, { smooth: 0.55 });
  body += curve(at(-10, 9), at(-17, 7), at(-23, 6), C.beardLight, 1.8);
  body += curve(at(-8, 1), at(-15, 0), at(-21, -2), C.beardLight, 1.8);
  body += poly([at(-1, 7), at(-2, 15), at(-6, 16), at(-7, 10), at(-4, 5)], C.beard, { smooth: 0.8, width: 1.6 });
  body += nose(at, C);
  body += earAndEye(at, C);
  // Хмурая бровь: к носу ниже.
  body += line(at(9.5, 4), at(7, 12.5), C.beard, 3.2);
  // Меховая шапка: колпак до затылка и толстый меховой околыш над бровью.
  const cap = [at(11, -16), at(19, -15), at(26, -8), at(29, 2), at(25, 10), at(18, 13)];
  body += poly(cap, C.hat, { smooth: 0.7 });
  body += curve(at(17, -10), at(24, -3), at(24, 6), C.hatDark, 1.8);
  const band = [at(4, -17), at(8, -2), at(12, 14), at(19, 14), at(15, -1), at(11, -18)];
  body += poly(band, C.hatFur, { smooth: 0.7 });
  for (const v of [-12, -6, 0, 6, 11]) body += line(at(7 + v * 0.22, v), at(13 + v * 0.22, v + 1), C.hatDark, 1.3);
  return body;
}

const ROBBER = { C, garment, head, cuff, weapon: club };

const base = { lean: 8, breath: 0, sash: 0, headTilt: -4 };

/** Стойка: сутулится, дубина в опущенной руке концом вперёд вниз. */
const IDLE = [0, 1, 2, 1].map((b) => ({
  ...base,
  breath: b * 0.8,
  near: { thigh: 12, knee: 14, foot: 0 },
  far: { thigh: -12, knee: 10, foot: 0 },
  nearArm: { upper: 16 - b, elbow: 30 + b * 2, clubAngle: 26 },
  farArm: { upper: -6 + b, elbow: 22 },
}));

/** Ход ноги за круг шага по 8 кадрам. */
const WALK_LEG = [
  { thigh: 24, knee: 4, foot: -10 },
  { thigh: 15, knee: 16, foot: 0 },
  { thigh: 4, knee: 12, foot: 0 },
  { thigh: -8, knee: 6, foot: 4 },
  { thigh: -20, knee: 14, foot: 16 },
  { thigh: -14, knee: 42, foot: 28 },
  { thigh: 5, knee: 46, foot: 12 },
  { thigh: 19, knee: 22, foot: -4 },
];
const WALK_LIFT = [0, 1, 2, 1, 0, 1, 2, 1];

/** Шаг: дальняя рука ходит навстречу дальней ноге, дубина покачивается. */
const WALK = WALK_LEG.map((nearLeg, i) => {
  const farLeg = WALK_LEG[(i + 4) % 8];
  return {
    ...base,
    lean: 10,
    lift: WALK_LIFT[i],
    sash: -4,
    near: nearLeg,
    far: farLeg,
    nearArm: { upper: 14 - 0.3 * nearLeg.thigh, elbow: 34, clubAngle: 26 },
    farArm: { upper: -0.8 * (farLeg.thigh - 2), elbow: 26 },
  };
});

const STANCE = { near: { thigh: 20, knee: 14, foot: 0 }, far: { thigh: -18, knee: 10, foot: 0 } };
const LUNGE = { near: { thigh: 38, knee: 26, foot: -4 }, far: { thigh: -26, knee: 6, foot: 6 } };

/** Обычный удар: дубина за голову, потом через верх вперёд вниз; передышка — дубина повисла. */
const BLOW = [
  { ...base, lean: -2, headTilt: 0, sash: 4, ...STANCE,
    nearArm: { upper: 155, elbow: 62, clubAngle: 60 }, farArm: { upper: 40, elbow: 40 } },
  { ...base, lean: -9, headTilt: -2, sash: 6, ...STANCE,
    nearArm: { upper: 176, elbow: 74, clubAngle: 66 }, farArm: { upper: 55, elbow: 30 } },
  { ...base, lean: 8, headTilt: 0, sash: -4, ...LUNGE, dx: 6, trail: { from: 250, to: 172, r: 140, width: 34 },
    nearArm: { upper: 150, elbow: 10, clubAngle: 14 }, farArm: { upper: 10, elbow: 30 } },
  { ...base, lean: 18, headTilt: 4, sash: -10, ...LUNGE, dx: 10, trail: { from: 172, to: 88, r: 140, width: 38 },
    nearArm: { upper: 80, elbow: 4, clubAngle: 12 }, farArm: { upper: -30, elbow: 30 } },
  { ...base, lean: 24, headTilt: 8, sash: -6, ...LUNGE, dx: 8,
    nearArm: { upper: 38, elbow: 12, clubAngle: 20 }, farArm: { upper: -10, elbow: 40 } },
];

const CROUCH = { near: { thigh: 52, knee: 84, foot: 0 }, far: { thigh: -36, knee: 40, foot: 10 } };
const SWEEP_LEGS = { near: { thigh: 62, knee: 60, foot: -6 }, far: { thigh: -48, knee: 22, foot: 14 } };

/** Особый удар: присел и отвёл дубину назад, потом взмах понизу во всю длину руки. */
const SWEEP = [
  { ...base, lean: 22, headTilt: -10, sash: 8, ...CROUCH,
    nearArm: { upper: -55, elbow: 14, clubAngle: 30 }, farArm: { upper: 50, elbow: 50 } },
  { ...base, lean: 28, headTilt: -14, sash: 10, ...CROUCH, dx: -6,
    nearArm: { upper: -88, elbow: 10, clubAngle: 30 }, farArm: { upper: 70, elbow: 40 } },
  { ...base, lean: 34, headTilt: -16, sash: -6, ...SWEEP_LEGS, dx: 8, trail: { from: -70, to: 20, r: 128, width: 30 },
    nearArm: { upper: -10, elbow: 4, clubAngle: 30 }, farArm: { upper: -40, elbow: 30 } },
  { ...base, lean: 36, headTilt: -14, sash: -12, ...SWEEP_LEGS, dx: 22, trail: { from: 20, to: 72, r: 128, width: 34 },
    nearArm: { upper: 52, elbow: 2, clubAngle: 22 }, farArm: { upper: -60, elbow: 20 } },
  { ...base, lean: 30, headTilt: -6, sash: -8, ...SWEEP_LEGS, dx: 18,
    nearArm: { upper: 40, elbow: 10, clubAngle: 30 }, farArm: { upper: -30, elbow: 40 } },
];

/** Отшатнулся: голова и корпус назад, дубина в отброшенной руке. */
const STAGGER = [
  { ...base, lean: -18, headTilt: -16, sash: 12, dx: -12, near: { thigh: 12, knee: 40, foot: 8 }, far: { thigh: -24, knee: 28, foot: 6 },
    nearArm: { upper: -40, elbow: 22, clubAngle: 40 }, farArm: { upper: 75, elbow: 30 } },
  { ...base, lean: -6, headTilt: -8, sash: 6, dx: -6, near: { thigh: 14, knee: 24, foot: 4 }, far: { thigh: -20, knee: 16, foot: 3 },
    nearArm: { upper: -12, elbow: 28, clubAngle: 32 }, farArm: { upper: 35, elbow: 35 } },
];

/** Лист кадров по 5 в строке: стойка 0–3, шаг 4–11, обычный удар 12–16, особый 17–21, отшатнулся 22–23. */
const SHEET = [...IDLE, ...WALK, ...BLOW, ...SWEEP, ...STAGGER];
const COLUMNS = 5;
/** Кадр 4 × 3 клетки: дубина в ударе уходит от середины почти на две клетки. */
const FRAME = [384, 288];

export function robberSvg() {
  return sheet(SHEET, ROBBER, FRAME, COLUMNS);
}

/** Отсвет не выходит за свой кадр: размытие ног не ложится на голову кадра строкой ниже. */
const CELL_REGION = `filterUnits="userSpaceOnUse" x="0" y="0" width="${FRAME[0]}" height="${FRAME[1]}"`;

/** Белый силуэт тех же кадров — вспышка от удара Ильи; светящаяся картинка прибавляет его к разбойнику. */
export function robberFlashSvg() {
  const id = uid("flash");
  const filter = `<filter id="${id}" ${CELL_REGION} color-interpolation-filters="sRGB"><feFlood flood-color="#fff6e8" result="c"/><feComposite in="c" in2="SourceAlpha" operator="in"/></filter>`;
  return sheet(SHEET, ROBBER, FRAME, COLUMNS, (body) => `<g filter="url(#${id})">${body}</g>`, filter);
}

/** Красный размытый ореол тех же кадров — особый замах. */
export function robberAuraSvg() {
  const id = uid("aura");
  // Сильнее по краю, слабее поверх самого разбойника: из размытого силуэта вычитается половина резкого.
  const filter = `<filter id="${id}" ${CELL_REGION} color-interpolation-filters="sRGB"><feMorphology in="SourceAlpha" operator="dilate" radius="5" result="d"/><feGaussianBlur in="d" stdDeviation="9" result="b"/><feComposite in="b" in2="SourceAlpha" operator="arithmetic" k1="0" k2="1" k3="-0.55" k4="0" result="ring"/><feFlood flood-color="#ff2a0a" result="c"/><feComposite in="c" in2="ring" operator="in"/></filter>`;
  return sheet(SHEET, ROBBER, FRAME, COLUMNS, (body) => `<g filter="url(#${id})">${body}</g>`, filter);
}
