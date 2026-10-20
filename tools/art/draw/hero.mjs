// Илья на общем скелете человека: рубаха с вышивкой и кушаком, порты, онучи, лапти, топор.
// Поза — углы суставов, кадр — поза, лист кадров — набор поз.
import { shade, num } from "./brush.mjs";
import { add, down, uid, limb, poly, curve, fist, face, nose, earAndEye, sheet } from "./figure.mjs";

const C = {
  shirt: "#efe6d2",
  shirtDark: "#c2b495",
  sleeve: "#efe6d2",
  sleeveDark: "#c2b495",
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

/** Длина топорища; кулак держит его у конца, конец торчит из кулака на `GRIP` от его середины. */
const HAFT = 60;
const GRIP = 12;

/**
 * Топор в кулаке. Топорище идёт поперёк кулака, лезвие — со стороны большого пальца,
 * куда показывает `axeAngle` от предплечья (плюс — к большому пальцу, он у Ильи впереди).
 * Кулак рисуется поверх топорища: пальцы обхватывают его, топорище выходит из кулака с обеих сторон.
 */
function axe(hand, foreDir, p) {
  const hd = down(p.upper + p.elbow + p.axeAngle);
  const butt = add(hand, hd, -GRIP);
  const top = add(hand, hd, HAFT - GRIP);
  const haft = limb(butt, top, 6.5, 5.5, C.haft, C.haftDark, false);
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
  let behind = haft.body + poly(blade, C.iron, { smooth: 0.35 });
  const e0 = add(add(top, hd, 11), side, s * 19);
  const e1 = add(add(top, hd, -19), side, s * 18);
  behind += `<path d="M${num(e0[0])},${num(e0[1])}L${num(e1[0])},${num(e1[1])}" stroke="#e7eef2" stroke-width="2.4" stroke-linecap="round"/>`;
  return { def: haft.def, behind, front: fist(hand, hd, foreDir, C) };
}

/** Обшлаг рукава с вышивкой. */
function cuff(wrist, foreDir, far) {
  const c0 = add(wrist, foreDir, -5);
  const n = [-foreDir[1], foreDir[0]];
  return `<path d="M${num(c0[0] + n[0] * 6.5)},${num(c0[1] + n[1] * 6.5)}L${num(c0[0] - n[0] * 6.5)},${num(c0[1] - n[1] * 6.5)}" stroke="${far ? shade(C.embroidery, -0.25) : C.embroidery}" stroke-width="3.5"/>`;
}

/** Рубаха до середины бедра, кушак с кистями, вышивка у ворота и подола. */
function garment(at, p) {
  const breath = p.breath ?? 0;
  const shirt = [
    at(-22, -24), at(-6, -22), at(14, -23), at(34, -25), at(50 + breath, -22), at(58 + breath, -13),
    at(61 + breath, 2), at(58 + breath, 17), at(46 + breath, 27), at(30, 28), at(12, 26), at(-6, 26), at(-23, 27),
  ];
  const id = uid("shirt");
  const defs = `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(at(40, -20)[0])}" y1="${num(at(40, -20)[1])}" x2="${num(at(10, 22)[0])}" y2="${num(at(10, 22)[1])}"><stop offset="0" stop-color="${C.shirtDark}"/><stop offset="0.5" stop-color="${C.shirt}"/><stop offset="1" stop-color="${shade(C.shirt, -0.06)}"/></linearGradient>`;
  let body = poly(shirt, `url(#${id})`, { smooth: 0.6 });
  // Складки подола и вышивка по подолу.
  body += curve(at(-21, -16), at(-23, 3), at(-22, 20), C.embroidery, 4).replace(' stroke-linecap="round"', "");
  body += `<path d="M${num(at(-4, 6)[0])},${num(at(-4, 6)[1])}L${num(at(-19, 4)[0])},${num(at(-19, 4)[1])}" stroke="${C.shirtDark}" stroke-width="2" stroke-linecap="round"/>`;
  // Ворот с вышивкой.
  body += curve(at(57 + breath, 5), at(52 + breath, 14), at(44 + breath, 19), C.embroidery, 4);
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
  return { defs, body };
}

/** Голова: окладистая русая борода, усы, волосы до затылка. */
function head(at) {
  let body = face(at, C);
  // Борода окладистая: от уха до груди.
  const beard = [at(-1, -4), at(-3, 9), at(-6, 15), at(-13, 15), at(-20, 12), at(-24, 6), at(-23, -2), at(-17, -7), at(-8, -7)];
  body += poly(beard, C.hair, { smooth: 0.75 });
  body += `<path d="M${num(at(-10, 8)[0])},${num(at(-10, 8)[1])}Q${num(at(-17, 6)[0])},${num(at(-17, 6)[1])} ${num(at(-22, 4)[0])},${num(at(-22, 4)[1])}M${num(at(-8, 1)[0])},${num(at(-8, 1)[1])}Q${num(at(-15, -1)[0])},${num(at(-15, -1)[1])} ${num(at(-20, -2)[0])},${num(at(-20, -2)[1])}" stroke="${C.hairLight}" stroke-width="1.8" fill="none" stroke-linecap="round"/>`;
  // Усы.
  body += poly([at(-1, 7), at(-2, 14), at(-5, 15), at(-7, 10), at(-4, 5)], C.hairDark, { smooth: 0.8, width: 1.6 });
  body += nose(at, C);
  // Волосы: шапка до затылка, прядь у виска.
  const hair = [at(-3, -15), at(6, -16), at(13, -12), at(17, -2), at(16, 8), at(12, 12), at(10, 9), at(11, 3), at(7, -2), at(3, -5), at(1, -9), at(-4, -12), at(-5, -16)];
  body += poly(hair, C.hair, { smooth: 0.7 });
  body += curve(at(12, -9), at(8, -3), at(-6, -12), C.hairLight, 2);
  body += earAndEye(at, C);
  body += `<path d="M${num(at(8.5, 6)[0])},${num(at(8.5, 6)[1])}L${num(at(8.5, 12)[0])},${num(at(8.5, 12)[1])}" stroke="${C.hairDark}" stroke-width="2.8" stroke-linecap="round"/>`;
  return body;
}

const ILYA = { C, garment, head, cuff, weapon: axe };

const base = { lean: 3, breath: 0, sash: 0, headTilt: 0 };
/** Рука опущена, топор держит за конец топорища: топор впереди у колена, лезвием вниз. */
const nearArmIdle = { upper: 4, elbow: 18, axeAngle: 45, axeFlip: true };

/** Стойка: 4 кадра дыхания. */
const IDLE = [0, 1, 2, 1].map((b) => ({
  ...base,
  breath: b * 0.8,
  near: { thigh: 5, knee: 4, foot: 0 },
  far: { thigh: -6, knee: 4, foot: 0 },
  nearArm: { ...nearArmIdle, upper: nearArmIdle.upper - b, elbow: nearArmIdle.elbow + b * 2 },
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

/** Ноги в прыжке у верхней точки: на них Илья рубит и защищается в воздухе. */
const AIR_LEGS = { near: JUMP[1].near, far: JUMP[1].far };

/** Ноги в бою на земле: ближняя впереди, дальняя позади. */
const STANCE = { near: { thigh: 18, knee: 10, foot: 0 }, far: { thigh: -16, knee: 8, foot: 0 } };
const LUNGE = { near: { thigh: 34, knee: 22, foot: -4 }, far: { thigh: -22, knee: 6, foot: 6 } };

/**
 * Удар с плеча: замах — топор за голову, лезвием назад вверх; удар — через верх вперёд вниз, лезвие
 * к врагу; возврат. Радиус следа — от плеча до лезвия.
 */
const CHOP = [
  { ...base, lean: -2, headTilt: -2, sash: 4, ...STANCE,
    nearArm: { upper: 150, elbow: 70, axeAngle: 80, axeFlip: true }, farArm: { upper: 35, elbow: 40 } },
  { ...base, lean: -7, headTilt: -4, sash: 6, ...STANCE,
    nearArm: { upper: 172, elbow: 78, axeAngle: 82, axeFlip: true }, farArm: { upper: 50, elbow: 30 } },
  { ...base, lean: 6, headTilt: 2, sash: -4, ...LUNGE, trail: { from: 250, to: 175, r: 128, width: 30 },
    nearArm: { upper: 148, elbow: 12, axeAngle: 22, axeFlip: true }, farArm: { upper: 10, elbow: 30 } },
  { ...base, lean: 15, headTilt: 6, sash: -10, ...LUNGE, trail: { from: 175, to: 92, r: 128, width: 34 },
    nearArm: { upper: 78, elbow: 4, axeAngle: 16, axeFlip: true }, farArm: { upper: -30, elbow: 30 } },
  { ...base, lean: 9, headTilt: 2, sash: -6, ...LUNGE,
    nearArm: { upper: 45, elbow: 35, axeAngle: 60, axeFlip: true }, farArm: { upper: -12, elbow: 30 } },
];

/** Парирование: топорище наискось перед лицом встречает дубину, потом отводит её вперёд вниз. */
const PARRY = [
  { ...base, lean: -6, headTilt: -6, sash: 6, near: { thigh: 22, knee: 26, foot: 0 }, far: { thigh: -20, knee: 12, foot: 2 },
    nearArm: { upper: 95, elbow: 70, axeAngle: -45, axeFlip: true }, farArm: { upper: 40, elbow: 95 } },
  { ...base, lean: 4, headTilt: 0, sash: -2, ...LUNGE, trail: { from: 150, to: 95, r: 118, width: 22 },
    nearArm: { upper: 70, elbow: 28, axeAngle: -8, axeFlip: true }, farArm: { upper: 20, elbow: 60 } },
];

/** Уклонение на месте: корпус отклоняется назад, вес на дальней ноге; рисунок чуть уходит назад. */
const DODGE = [
  { ...base, lean: -24, headTilt: 8, sash: 10, dx: -14, near: { thigh: 30, knee: 4, foot: -8 }, far: { thigh: -6, knee: 42, foot: 4 },
    nearArm: { upper: 30, elbow: 72, axeAngle: 78, axeFlip: true }, farArm: { upper: -55, elbow: 20 } },
  { ...base, lean: -11, headTilt: 3, sash: 6, dx: -6, near: { thigh: 24, knee: 8, foot: -4 }, far: { thigh: -10, knee: 24, foot: 2 },
    nearArm: { upper: 20, elbow: 75, axeAngle: 85, axeFlip: true }, farArm: { upper: -30, elbow: 25 } },
];

/** Пропущенный удар: голова откинута, руки врозь, шаг назад. */
const HURT = [
  { ...base, lean: -20, headTilt: -22, sash: 14, dx: -12, lift: -4, near: { thigh: 14, knee: 44, foot: 8 }, far: { thigh: -24, knee: 30, foot: 6 },
    nearArm: { upper: -50, elbow: 18, axeAngle: 70, axeFlip: true }, farArm: { upper: 85, elbow: 25 } },
  { ...base, lean: -9, headTilt: -9, sash: 8, dx: -6, near: { thigh: 12, knee: 26, foot: 4 }, far: { thigh: -18, knee: 18, foot: 3 },
    nearArm: { upper: -22, elbow: 28, axeAngle: 62, axeFlip: true }, farArm: { upper: 45, elbow: 40 } },
];

const inAir = (poses) => poses.map((p) => ({ ...p, ...AIR_LEGS, dx: 0 }));

/**
 * Лист кадров по 6 в строке: стойка 0–3, бег 4–11, прыжок 12–14, удар на земле 15–19 и в прыжке 20–24,
 * парирование 25–26 и 27–28, уклонение 29–30 и 31–32, пропущенный удар 33–34 и 35–36.
 */
const SHEET = [...IDLE, ...RUN, ...JUMP, ...CHOP, ...inAir(CHOP), ...PARRY, ...inAir(PARRY), ...DODGE, ...inAir(DODGE), ...HURT, ...inAir(HURT)];
const COLUMNS = 6;
/** Кадр 3,5 × 3 клетки: топор в ударе уходит от середины на полторы клетки. */
const FRAME = [336, 288];

export function sheetSvg() {
  return sheet(SHEET, ILYA, FRAME, COLUMNS);
}

