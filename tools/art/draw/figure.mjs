// Скелет человека для персонажей платформера: поза — углы суставов, кадр — поза, лист кадров — набор поз.
// Смотрит вправо; ближние к зрителю рука и нога — левые, дальние темнее и позади. Одежду, голову и
// оружие рисует сам персонаж — `look`; длины частей и суставы у всех общие.
import { svg, capsule, smoothClosed, shade, num } from "./brush.mjs";

export const OUTLINE = "#2b1f17";

/** Длины частей в точках: рост около 186, семь с половиной голов. */
export const L = { torso: 58, neck: 5, upperArm: 33, foreArm: 29, thigh: 44, shin: 42, ankle: 8, foot: 31, heel: 8 };

export const rad = (d) => (d * Math.PI) / 180;
/** Направление по углу от «прямо вниз», плюс — вперёд (вправо). */
export const down = (deg) => [Math.sin(rad(deg)), Math.cos(rad(deg))];
export const add = (p, v, k) => [p[0] + v[0] * k, p[1] + v[1] * k];

let gid = 0;
/** Свой номер для перехода цвета или фильтра: в листе много кадров, имена не должны совпасть. */
export const uid = (prefix) => `${prefix}${gid++}`;

/** Часть тела палкой с переходом цвета поперёк: светлая сторона — к свету слева сверху. */
export function limb(a, b, wa, wb, light, dark, far) {
  const id = uid("g");
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
  return { def, body: `<path d="${capsule(a, b, wa, wb)}" fill="url(#${id})" stroke="${OUTLINE}" stroke-width="2.2" stroke-linejoin="round"/>` };
}

export function poly(points, fill, { stroke = OUTLINE, width = 2.2, smooth = 0, opacity = 1 } = {}) {
  const d = smooth ? smoothClosed(points, smooth) : `M${points.map(([x, y]) => `${num(x)},${num(y)}`).join("L")}Z`;
  return `<path d="${d}" fill="${fill}" stroke="${stroke}" stroke-width="${width}" stroke-linejoin="round" opacity="${opacity}"/>`;
}

export function line(a, b, color, width) {
  return `<path d="M${num(a[0])},${num(a[1])}L${num(b[0])},${num(b[1])}" stroke="${color}" stroke-width="${width}" stroke-linecap="round"/>`;
}

export function curve(a, c, b, color, width, fill = "none") {
  return `<path d="M${num(a[0])},${num(a[1])}Q${num(c[0])},${num(c[1])} ${num(b[0])},${num(b[1])}" stroke="${color}" stroke-width="${width}" fill="${fill}" stroke-linecap="round"/>`;
}

/** Нога: бедро в портах, голень в онучах с оборами, лапоть. */
function leg(hip, p, far, C) {
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

/**
 * Кулак вокруг рукояти, идущей по направлению `hd`. Тыльная сторона — к запястью, костяшки и пальцы —
 * от него; большой палец лежит сверху, со стороны `hd`. Рисуется поверх рукояти.
 */
export function fist(hand, hd, foreDir, C) {
  let f = [-hd[1], hd[0]];
  if (f[0] * foreDir[0] + f[1] * foreDir[1] < 0) f = [-f[0], -f[1]];
  const at = (u, v) => add(add(hand, hd, u), f, v);
  const outline = [at(-8, -6), at(5, -6.5), at(9.5, -2), at(9.5, 4), at(6.5, 8), at(2, 8.5), at(-2.5, 8.5), at(-7, 8), at(-9.5, 4), at(-9.5, -2)];
  let body = poly(outline, C.skin, { smooth: 0.55 });
  // Тень пальцев под костяшками и складки между ними.
  body += `<path d="M${num(at(-7.5, 3)[0])},${num(at(-7.5, 3)[1])}L${num(at(7.5, 3)[0])},${num(at(7.5, 3)[1])}" stroke="${C.skinDark}" stroke-width="1.6" stroke-linecap="round"/>`;
  for (const u of [-4.6, -0.3, 4]) {
    body += `<path d="M${num(at(u, 3.5)[0])},${num(at(u, 3.5)[1])}L${num(at(u, 8)[0])},${num(at(u, 8)[1])}" stroke="${OUTLINE}" stroke-width="1.5" stroke-linecap="round"/>`;
  }
  body += `<path d="M${num(at(4, -6)[0])},${num(at(4, -6)[1])}Q${num(at(10, -3)[0])},${num(at(10, -3)[1])} ${num(at(8, 2.5)[0])},${num(at(8, 2.5)[1])}" stroke="${OUTLINE}" stroke-width="1.5" fill="none" stroke-linecap="round"/>`;
  return body;
}

/** Рука: рукав со своим обшлагом, кулак; в ближней — оружие персонажа. */
function arm(shoulder, p, far, look, armed) {
  const C = look.C;
  const elbow = add(shoulder, down(p.upper), L.upperArm);
  const foreDir = down(p.upper + p.elbow);
  const wrist = add(elbow, foreDir, L.foreArm);
  const hand = add(wrist, foreDir, 6);
  const upper = limb(shoulder, elbow, 22, 18, C.sleeve, C.sleeveDark, far);
  const fore = limb(elbow, wrist, 18, 15, C.sleeve, C.sleeveDark, far);
  let defs = upper.def + fore.def;
  let body = upper.body + fore.body + look.cuff(wrist, foreDir, far);
  if (armed) {
    // Оружие позади руки, кулак поверх рукояти.
    const w = look.weapon(hand, foreDir, p);
    return { defs: defs + w.def, body: w.behind + body + w.front, hand };
  }
  body += `<circle cx="${num(hand[0])}" cy="${num(hand[1])}" r="8.5" fill="${far ? shade(C.skin, -0.2) : C.skin}" stroke="${OUTLINE}" stroke-width="2.2"/>`;
  return { defs, body, hand };
}

/**
 * Туловище по наклону `lean`: `at(u, v)` — точка на `u` выше таза вдоль спины и на `v` вперёд.
 * Одежду рисует персонаж.
 */
function torso(hip, p, look) {
  const up = [Math.sin(rad(p.lean)), -Math.cos(rad(p.lean))];
  const fwd = [-up[1], up[0]];
  const at = (u, v) => [hip[0] + up[0] * u + fwd[0] * v, hip[1] + up[1] * u + fwd[1] * v];
  const breath = p.breath ?? 0;
  const neck = at(L.torso + breath, 6);
  const shoulder = at(L.torso - 10 + breath, -2);
  const garment = look.garment(at, p);
  return { defs: garment.defs, body: garment.body, neck, shoulder };
}

/** Голова в профиль: `at(u, v)` — точка на `u` к макушке и на `v` к лицу от середины головы. */
function head(neck, p, look) {
  const tilt = p.headTilt ?? 0;
  const hu = [Math.sin(rad(p.lean + tilt)), -Math.cos(rad(p.lean + tilt))];
  const hf = [-hu[1], hu[0]];
  const c = add(add(neck, hu, 17), hf, 2);
  const at = (u, v) => [c[0] + hu[0] * u + hf[0] * v, c[1] + hu[1] * u + hf[1] * v];
  return look.head(at, p);
}

/** Шея и лицо в профиль — общие у всех; волосы, борода и шапка — у персонажа. */
export function face(at, C) {
  let body = poly([at(-20, -6), at(-6, -7), at(-6, 6), at(-20, 7)], C.skinDark, { width: 2 });
  const outline = [at(12, -11), at(15, 0), at(12, 9), at(6, 12), at(3, 13.5), at(0, 15.5), at(-2, 13), at(-6, 13), at(-12, 9), at(-14, 0), at(-10, -11), at(0, -14)];
  body += poly(outline, C.skin, { smooth: 0.7 });
  return body;
}

/**
 * След взмаха: светлый серп вокруг плеча от угла `from` к углу `to` (углы — как у суставов, от «прямо
 * вниз», плюс — вперёд), снаружи по радиусу `r`, толщиной до `width` у `to` и сходящий на нет к `from`.
 */
export function swoosh(center, { from, to, r, width }, color = "#fffaf0") {
  const n = 14;
  const outer = [];
  const inner = [];
  for (let i = 0; i <= n; i++) {
    const k = i / n;
    const a = from + (to - from) * k;
    const d = down(a);
    outer.push(add(center, d, r));
    inner.push(add(center, d, r - width * k * k));
  }
  const id = uid("sw");
  const a0 = outer[0];
  const a1 = outer[n];
  const defs = `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${num(a0[0])}" y1="${num(a0[1])}" x2="${num(a1[0])}" y2="${num(a1[1])}"><stop offset="0" stop-color="${color}" stop-opacity="0"/><stop offset="1" stop-color="${color}" stop-opacity="0.75"/></linearGradient>`;
  const pts = [...outer, ...inner.reverse()];
  return `<defs>${defs}</defs><path d="M${pts.map(([x, y]) => `${num(x)},${num(y)}`).join("L")}Z" fill="url(#${id})"/>`;
}

/** Нос поверх бороды и усов. */
export function nose(at, C) {
  return `<path d="M${num(at(4, 13)[0])},${num(at(4, 13)[1])}Q${num(at(1, 18.5)[0])},${num(at(1, 18.5)[1])} ${num(at(-1, 14)[0])},${num(at(-1, 14)[1])}" stroke="${OUTLINE}" stroke-width="1.6" fill="${C.skin}"/>`;
}

/** Ухо и глаз поверх волос; бровь — у персонажа. */
export function earAndEye(at, C) {
  let body = `<ellipse cx="${num(at(0, -5)[0])}" cy="${num(at(0, -5)[1])}" rx="3.2" ry="4.6" fill="${C.skinDark}" stroke="${OUTLINE}" stroke-width="1.6"/>`;
  body += `<ellipse cx="${num(at(5, 9)[0])}" cy="${num(at(5, 9)[1])}" rx="1.7" ry="2.1" fill="${OUTLINE}"/>`;
  return body;
}

/**
 * Рисует одну позу в кадре `frame` × `height`: ступни у нижней кромки, середина таза — посередине кадра,
 * сдвинутая на `dx`; `lift` поднимает над землёй.
 */
export function pose(p, look, frame = [288, 288]) {
  const ground = frame[1] - 3;
  const hip0 = [frame[0] / 2 + (p.dx ?? 0), 100];
  const legNear0 = leg(hip0, p.near, false, look.C);
  const legFar0 = leg(hip0, p.far, true, look.C);
  const lowest = Math.max(legNear0.lowest, legFar0.lowest);
  const hip = [hip0[0], hip0[1] + (ground - lowest) - (p.lift ?? 0)];
  const legNear = leg(hip, p.near, false, look.C);
  const legFar = leg(hip, p.far, true, look.C);
  const t = torso(hip, p, look);
  const armFar = arm(t.shoulder, p.farArm, true, look, false);
  const armNear = arm(t.shoulder, p.nearArm, false, look, true);
  const h = head(t.neck, p, look);
  const defs = legFar.defs + legNear.defs + t.defs + armFar.defs + armNear.defs;
  // След взмаха — позади ближней руки с оружием.
  const trail = p.trail ? swoosh(t.shoulder, p.trail) : "";
  const body = armFar.body + legFar.body + legNear.body + t.body + h + trail + armNear.body;
  return { defs, body };
}

/**
 * Лист кадров: позы сеткой по `columns` в строке, кадр `frame` точек. `wrap(body, x, y, w, h)`
 * оборачивает кадр — так из тех же поз получаются светящиеся силуэты.
 */
export function sheet(poses, look, frame, columns, wrap = (body) => body, extraDefs = "") {
  const rows = Math.ceil(poses.length / columns);
  let defs = extraDefs;
  let body = "";
  poses.forEach((p, i) => {
    const { defs: d, body: b } = pose(p, look, frame);
    defs += d;
    const x = (i % columns) * frame[0];
    const y = Math.floor(i / columns) * frame[1];
    body += `<g transform="translate(${x},${y})">${wrap(b, frame[0], frame[1])}</g>`;
  });
  return svg(columns * frame[0], rows * frame[1], body, defs);
}
