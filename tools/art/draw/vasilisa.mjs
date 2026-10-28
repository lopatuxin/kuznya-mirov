// Василиса Премудрая на общем скелете человека: белая рубаха с вышитыми рукавами, красный сарафан с белым
// узором, тканый пояс, коса до пояса, очелье, онучи, лапти, посох. Начальная одежда — как на «Распутье»
// (art/platformer/start/vasilisa.png). Позы и раскладка листа — Ильи; ростом она чуть ниже.
import { shade, num } from "./brush.mjs";
import { L, add, down, uid, limb, poly, curve, line, fist, face, nose, earAndEye, sheet } from "./figure.mjs";
import { SHEET, COLUMNS, FRAME } from "./hero.mjs";

const C = {
  blouse: "#f4ede1",
  blouseDark: "#cbbda4",
  sleeve: "#f4ede1",
  sleeveDark: "#cbbda4",
  embroidery: "#c0392b",
  sarafan: "#cc5530",
  sarafanDark: "#8c3219",
  ornament: "#f3e4cf",
  belt: "#b8843f",
  beltDark: "#7d5522",
  trousers: "#efe6d6",
  trousersDark: "#bdb09a",
  wrap: "#efe6d6",
  wrapDark: "#bdb09a",
  strap: "#b5a68c",
  lapti: "#b5793e",
  laptiDark: "#7a4c20",
  skin: "#f0c4a4",
  skinDark: "#cb917a",
  hair: "#a8602c",
  hairLight: "#d18d4c",
  hairDark: "#6c3816",
  band: "#c0392b",
  wood: "#9a6a3a",
  woodDark: "#5e3d1e",
};

/** Ростом ниже Ильи: весь рисунок сжат к ступням. */
const SCALE = 0.94;
/** Посох во всю длину; `p.staffOut` — сколько его выходит из кулака туда, куда показывает `axeAngle`. */
const STAFF = 158;

function staff(hand, foreDir, p) {
  const hd = down(p.upper + p.elbow + p.axeAngle);
  const out = p.staffOut ?? 75;
  const tip = add(hand, hd, out);
  const butt = add(hand, hd, out - STAFF);
  const pole = limb(butt, tip, 7, 6, C.wood, C.woodDark, false);
  // Резное навершие: утолщение и два кольца.
  const knob = add(tip, hd, 2);
  let behind = pole.body + `<circle cx="${num(knob[0])}" cy="${num(knob[1])}" r="6" fill="${C.wood}" stroke="#2b1f17" stroke-width="2"/>`;
  const side = [hd[1], -hd[0]];
  for (const k of [10, 16]) {
    const r = add(tip, hd, -k);
    behind += line(add(r, side, -4.5), add(r, side, 4.5), C.woodDark, 2.2);
  }
  return { def: pole.def, behind, front: fist(hand, hd, foreDir, C) };
}

/** Обшлаг рукава: вышитая полоса у запястья. */
function cuff(wrist, foreDir, far) {
  const c0 = add(wrist, foreDir, -6);
  const n = [-foreDir[1], foreDir[0]];
  return `<path d="M${num(c0[0] + n[0] * 7.5)},${num(c0[1] + n[1] * 7.5)}L${num(c0[0] - n[0] * 7.5)},${num(c0[1] - n[1] * 7.5)}" stroke="${far ? shade(C.embroidery, -0.25) : C.embroidery}" stroke-width="5"/>`;
}

/** Точка на голени ноги `leg` на доле `k` от колена к щиколотке — так подол идёт за ногами. */
function shinPoint(hip, leg, k) {
  const knee = add(hip, down(leg.thigh), L.thigh);
  return add(knee, down(leg.thigh - leg.knee), L.shin * k);
}

/** Рубаха под сарафаном, сарафан на лямках, пояс; подол расходится вслед за ногами и не доходит до щиколоток. */
function garment(at, p) {
  const breath = p.breath ?? 0;
  const hip = at(0, 0);
  let defs = "";
  let body = "";
  // Рубаха: видна выше сарафана и в проймах.
  body += poly([at(30, -18), at(46, -20), at(56 + breath, -14), at(60 + breath, -2), at(57 + breath, 12), at(48 + breath, 19), at(30, 18)], C.blouse, { smooth: 0.6 });
  body += curve(at(58 + breath, 2), at(56 + breath, 8), at(52 + breath, 12), C.embroidery, 2.4);
  // Подол: по ногам, расходится колоколом.
  const hems = [shinPoint(hip, p.near, 0.8), shinPoint(hip, p.far, 0.8)];
  const back = Math.min(hems[0][0], hems[1][0]) - 22;
  const front = Math.max(hems[0][0], hems[1][0]) + 22;
  const hemY = Math.max(hems[0][1], hems[1][1]);
  const sw = (p.sash ?? 0) * 0.5;
  const hemBack = [back + sw, hemY - 4];
  const hemFront = [front + sw * 0.5, hemY - 2];
  const hemMid = [(back + front) / 2 + sw * 0.7, hemY + 2];
  const id = uid("saraf");
  defs += `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(back)}" y1="0" x2="${num(front)}" y2="0"><stop offset="0" stop-color="${C.sarafanDark}"/><stop offset="0.55" stop-color="${C.sarafan}"/><stop offset="1" stop-color="${shade(C.sarafan, -0.08)}"/></linearGradient>`;
  // Колени, поднятые в прыжке, подол обтягивает спереди и сзади, иначе они торчат из-под него.
  const knees = [p.near, p.far].map((leg) => add(hip, down(leg.thigh), L.thigh));
  const kneeFront = knees.reduce((a, b) => (b[0] > a[0] ? b : a));
  const kneeBack = knees.reduce((a, b) => (b[0] < a[0] ? b : a));
  const dress = [
    at(48 + breath, -14), at(48 + breath, 12), at(30, 18), at(12, 22),
    [Math.max(kneeFront[0] + 16, (at(12, 22)[0] + hemFront[0]) / 2), kneeFront[1]],
    hemFront, hemMid, hemBack,
    [Math.min(kneeBack[0] - 16, (at(12, -22)[0] + hemBack[0]) / 2), kneeBack[1]],
    at(12, -22), at(30, -17),
  ];
  body += poly(dress, `url(#${id})`, { smooth: 0.45 });
  // Узор: полоса по переду от пояса до подола и кайма по подолу.
  const frontTop = at(26, 17);
  body += `<path d="M${num(frontTop[0])},${num(frontTop[1])}Q${num((frontTop[0] + hemFront[0]) / 2 + 3)},${num((frontTop[1] + hemFront[1]) / 2)} ${num(hemFront[0] - 6)},${num(hemFront[1] - 3)}" stroke="${C.ornament}" stroke-width="4" fill="none"/>`;
  const band = [hemBack, hemMid, hemFront].map(([x, y]) => [x, y - 9]);
  body += `<path d="M${num(band[0][0] + 3)},${num(band[0][1])}Q${num(band[1][0])},${num(band[1][1] + 2)} ${num(band[2][0] - 4)},${num(band[2][1])}" stroke="${C.ornament}" stroke-width="5" fill="none"/>`;
  for (let k = 0.12; k < 0.95; k += 0.16) {
    const x = band[0][0] + (band[2][0] - band[0][0]) * k;
    const y = band[0][1] + (band[2][1] - band[0][1]) * k + Math.sin(k * Math.PI) * 2;
    body += poly([[x, y - 2.6], [x + 2.6, y], [x, y + 2.6], [x - 2.6, y]], C.embroidery, { width: 0 });
  }
  // Лямка сарафана через плечо.
  body += line(at(48 + breath, -6), at(56 + breath, -6), C.sarafanDark, 4);
  // Тканый пояс.
  body += poly([at(32, -18), at(32, 19), at(26, 19), at(26, -18)], C.belt, { width: 1.8 });
  body += line(at(29, -15), at(29, 16), C.beltDark, 1.2);
  return { defs, body };
}

/** Голова: волосы на прямой пробор, красное очелье, коса по спине до пояса. */
function head(at, p) {
  let body = "";
  // Коса висит от затылка вниз и отстаёт при беге, как кушак.
  const nape = at(-6, -12);
  const sw = p.sash ?? 0;
  const links = 7;
  let prev = nape;
  const segs = [];
  for (let i = 1; i <= links; i++) {
    const k = i / links;
    const pt = [nape[0] - 4 - sw * 0.9 * k - k * 6, nape[1] + k * 74 - Math.abs(sw) * 0.4 * k];
    segs.push([prev, pt, 11 - k * 4]);
    prev = pt;
  }
  for (const [a, b, w] of segs) {
    body += `<ellipse cx="${num((a[0] + b[0]) / 2)}" cy="${num((a[1] + b[1]) / 2)}" rx="${num(w / 2 + 1)}" ry="${num(Math.hypot(b[0] - a[0], b[1] - a[1]) / 2 + 2)}" transform="rotate(${num((Math.atan2(b[1] - a[1], b[0] - a[0]) * 180) / Math.PI - 90)} ${num((a[0] + b[0]) / 2)} ${num((a[1] + b[1]) / 2)})" fill="${C.hair}" stroke="#2b1f17" stroke-width="1.6"/>`;
  }
  const end = segs[segs.length - 1][1];
  body += line(add(end, [1, 0], -4), add(end, [1, 0], 4), C.band, 3.5);
  body += poly([[end[0] - 4, end[1] + 2], [end[0] + 4, end[1] + 2], [end[0] + 2, end[1] + 12], [end[0] - 3, end[1] + 12]], C.hairLight, { width: 1.4 });
  body += face(at, C);
  body += nose(at, C);
  // Губы.
  body += line(at(-4.5, 12.5), at(-4, 9.5), C.embroidery, 1.6);
  // Волосы гладко назад, на затылке узел перед косой.
  const hair = [at(-4, -15), at(6, -17), at(14, -13), at(18, -3), at(17, 7), at(14, 11), at(12, 8), at(11, 1), at(7, -3), at(2, -6), at(-4, -8), at(-9, -12)];
  body += poly(hair, C.hair, { smooth: 0.7 });
  body += curve(at(15, -6), at(10, -9), at(-2, -14), C.hairLight, 1.8);
  // Очелье поперёк головы над лбом.
  body += line(at(16.5, 6), at(10, -16), C.band, 3.6);
  body += earAndEye(at, C);
  // Тонкая бровь и ресницы.
  body += line(at(8.8, 6.5), at(8.5, 11.5), C.hairDark, 1.6);
  body += line(at(5.5, 10.5), at(6.5, 12.5), "#2b1f17", 1.2);
  return body;
}

const VASILISA = { C, garment, head, cuff, weapon: staff };

/** Стойка с посохом: посох стоймя у ноги, нижний конец у земли. */
const STAND_ARM = { upper: 14, elbow: 46, axeAngle: 120, staffOut: 69 };
/** След взмаха — по концу посоха: плечо, рука и 75 точек посоха из кулака. */
const TRAIL_R = 143;
const POSES = SHEET.map((p, i) => {
  if (i < 4) return { ...p, nearArm: { ...STAND_ARM, upper: STAND_ARM.upper + (p.nearArm.upper - 4) } };
  return p.trail ? { ...p, trail: { ...p.trail, r: TRAIL_R } } : p;
});

function shrink(body, w, h) {
  const gx = w / 2;
  const gy = h - 3;
  return `<g transform="translate(${gx},${gy}) scale(${SCALE}) translate(${-gx},${-gy})">${body}</g>`;
}

export function vasilisaSvg() {
  return sheet(POSES, VASILISA, FRAME, COLUMNS, shrink);
}
