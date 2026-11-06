// Поправки ног для «подъём_зомби_по_склону»: подушечка и пальцы на земле, голеностоп в живых пределах.
// node solve.mjs <glb> <клип> <вывод bones.json>
import fs from 'fs';
import { loadRig, qmul, qinv, qrot, axq, add, sub, mul, dot, cross, norm, unit, fromTo, slerp, angle, smooth, toEuler, ground, tangent, normal } from './rig.mjs';

const [, , file, clipName, outJson] = process.argv;
const R = loadRig(file, clipName);
const { len, locals, globals, id, parent } = R;

const SOLE = 0.022; // от суставов подушечки и кончика пальцев до подошвы (по стоящей ступне в конце клипа)
const MAX_ANKLE = 72; // наибольший угол «голень — стопа»: стоя 55, сильнее согнуть голеностоп нельзя
const SWING_ANKLE = 38; // стопа в воздухе: расслабленно оттянута назад
const FPS = 60;
const KNEE = 0.03; // сустав колена над землёй: ниже — колено уходит в землю

const copy = (L) => L.map((l) => ({ ...l }));
function aim(L, i, child, dir) {
  const G = globals(L);
  const cur = sub(G[child].p, G[i].p);
  const r = qmul(fromTo(cur, dir), G[i].r);
  L[i].r = unit(qmul(qinv(G[parent[i]].r), r));
}
function legIK(L, side, target) {
  const up = id(side + 'UpLeg'), kn = id(side + 'Leg'), an = id(side + 'Foot');
  const G = globals(L);
  const H = G[up].p, K = G[kn].p, A = G[an].p;
  const l1 = norm(sub(K, H)), l2 = norm(sub(A, K));
  const d = sub(target, H);
  const dl = Math.min(norm(d), (l1 + l2) * 0.999);
  const dir = unit(d);
  const aH = Math.acos(Math.max(-1, Math.min(1, (l1 * l1 + dl * dl - l2 * l2) / (2 * l1 * dl))));
  let pole = sub(K, H);
  pole = unit(sub(pole, mul(dir, dot(pole, dir))));
  const K2 = add(H, add(mul(dir, l1 * Math.cos(aH)), mul(pole, l1 * Math.sin(aH))));
  aim(L, up, kn, sub(K2, H));
  aim(L, kn, an, sub(add(H, mul(dir, dl)), globals(L)[kn].p));
}
// Стопа в воздухе: голень как в клипе, стопа под SWING_ANKLE к голени в своей плоскости, пальцы как в клипе.
function swingPose(L, side) {
  const kn = id(side + 'Leg'), an = id(side + 'Foot'), ba = id(side + 'ToeBase');
  const G = globals(L);
  const shin = unit(sub(G[an].p, G[kn].p));
  let ax = cross(shin, sub(G[ba].p, G[an].p));
  ax = norm(ax) < 1e-6 ? [1, 0, 0] : unit(ax);
  aim(L, an, ba, qrot(axq(ax, (SWING_ANKLE * Math.PI) / 180), shin));
}
// Стопа на земле: кончик пальцев на подошве под своим местом, пальцы вдоль земли, пятка поднята ровно настолько,
// чтобы голеностоп не согнулся сильнее MAX_ANKLE, нога дотягивается сгибом колена.
function stancePose(L, side, G0) {
  const kn = id(side + 'Leg'), an = id(side + 'Foot'), ba = id(side + 'ToeBase'), te = id(side + 'Toe_End');
  const lf = norm(sub(G0[ba].p, G0[an].p)), le = norm(sub(G0[te].p, G0[ba].p));
  const e = G0[te].p;
  const tip = add([e[0], ground(e[2]), e[2]], mul(normal(e[2]), SOLE));
  const tg = tangent(e[2] - le * 0.5);
  const guess = sub(tip, mul(tg, le));
  const ball = add([guess[0], ground(guess[2]), guess[2]], mul(normal(guess[2]), SOLE));
  const up = normal(ball[2]);
  const k0 = G0[kn].p;
  const kneeFloor = Math.min(KNEE, k0[1] - ground(k0[2]));
  let best = null;
  for (let phi = 30; phi <= 125; phi += 1) {
    const r = (phi * Math.PI) / 180;
    const A = add(ball, mul(add(mul(tg, -Math.cos(r)), mul(up, Math.sin(r))), lf));
    const T = copy(L);
    legIK(T, side, A);
    const S = globals(T);
    const miss = norm(sub(S[an].p, A));
    const a = angle(sub(S[an].p, S[kn].p), sub(ball, A));
    const knee = S[kn].p[1] - ground(S[kn].p[2]);
    if (knee < kneeFloor) continue;
    const fit = { phi, A, miss, a };
    if (miss < 0.003 && a <= MAX_ANKLE) { best = fit; break; }
    if (!best || miss + Math.max(0, a - MAX_ANKLE) * 0.002 < best.miss + Math.max(0, best.a - MAX_ANKLE) * 0.002) best = fit;
  }
  legIK(L, side, best.A);
  aim(L, an, ba, sub(ball, globals(L)[an].p));
  aim(L, ba, te, sub(tip, globals(L)[ba].p));
  return best;
}

// Если смесь опустила подушечку или кончик пальцев в землю — довернуть стопу и пальцы вверх.
function lift(L, side) {
  const an = id(side + 'Foot'), ba = id(side + 'ToeBase'), te = id(side + 'Toe_End');
  for (const [bone, child] of [[an, ba], [ba, te]]) {
    for (let n = 0; n < 4; n++) {
      const G = globals(L);
      const p = G[child].p;
      const low = ground(p[2]) + SOLE * normal(p[2])[1] - p[1];
      if (low <= 0.0005) break;
      const from = G[bone].p;
      const want = add(p, [0, low, 0]);
      aim(L, bone, child, sub(want, from));
    }
  }
}

const frames = Math.round(len * FPS);
const share = (t) => Math.min(smooth(0, 0.35, t), 1 - smooth(len - 0.3, len, t)); // у краёв клипа — к исходной позе
// Вес «на земле» по зазору кончика пальцев: опора расширена на DILATE кадров и сглажена по времени,
// чтобы стопа заранее готовилась к постановке и отлипала от земли постепенно.
const DILATE = 6, BLUR = 10;
const raw = { Left: [], Right: [] };
for (let f = 0; f <= frames; f++) {
  const G = globals(locals(Math.min(len, f / FPS)));
  for (const side of ['Left', 'Right']) {
    const e = G[id(side + 'Toe_End')].p;
    raw[side].push(1 - smooth(0.05, 0.1, e[1] - ground(e[2])));
  }
}
const weight = {};
for (const side of ['Left', 'Right']) {
  const r = raw[side];
  const wide = r.map((_, i) => Math.max(...r.slice(Math.max(0, i - DILATE), i + DILATE + 1)));
  weight[side] = wide.map((_, i) => { const part = wide.slice(Math.max(0, i - BLUR), i + BLUR + 1); return part.reduce((a, b) => a + b, 0) / part.length; });
}
const out = {};
const last = {};
const log = [];
for (let f = 0; f <= frames; f++) {
  const t = Math.min(len, f / FPS);
  const base = locals(t);
  const G0 = globals(base);
  const L = copy(base);
  const k = share(t);
  const row = { t: +t.toFixed(3) };
  for (const side of ['Left', 'Right']) {
    const w = weight[side][f];
    const Sw = copy(L);
    swingPose(Sw, side);
    const St = copy(L);
    const fit = stancePose(St, side, G0);
    for (const nm of ['UpLeg', 'Leg', 'Foot', 'ToeBase']) {
      const i = id(side + nm);
      L[i] = { ...L[i], r: slerp(base[i].r, slerp(Sw[i].r, St[i].r, w), k) };
    }
    lift(L, side);
    row[side] = { w: +w.toFixed(2), phi: fit.phi, miss: +fit.miss.toFixed(4), ankle: +fit.a.toFixed(0) };
  }
  for (const side of ['Left', 'Right'])
    for (const nm of ['UpLeg', 'Leg', 'Foot', 'ToeBase']) {
      const i = id(side + nm);
      const key = 'mixamorig:' + side + nm;
      let e = toEuler(qmul(qinv(base[i].r), L[i].r));
      if (last[key]) e = e.map((v, c) => { while (v - last[key][c] > 180) v -= 360; while (v - last[key][c] < -180) v += 360; return v; });
      last[key] = e;
      (out[key] ??= []).push({ at: +(t / len).toFixed(5), turn: e.map((v) => +v.toFixed(2)) });
    }
  log.push(row);
}
fs.writeFileSync(outJson, JSON.stringify(out));
fs.writeFileSync(outJson.replace(/\.json$/, '.log.json'), JSON.stringify(log));
console.log('кадров', frames + 1, 'длина', len);
for (const r of log.filter((_, i) => i % 6 == 0)) console.log(r.t, JSON.stringify(r.Left), JSON.stringify(r.Right));
