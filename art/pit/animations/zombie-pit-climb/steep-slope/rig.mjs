// Чтение клипа GLB и поза скелета по кадру — общая часть расчёта стоп.
import fs from 'fs';

export function loadRig(file, clipName) {
  const b = fs.readFileSync(file);
  const n = b.readUInt32LE(12);
  const j = JSON.parse(b.slice(20, 20 + n));
  const binStart = 20 + n + 8;
  const names = j.nodes.map((x) => x.name);
  const acc = (i) => {
    const a = j.accessors[i];
    const bv = j.bufferViews[a.bufferView];
    const comps = { SCALAR: 1, VEC3: 3, VEC4: 4 }[a.type];
    const off = binStart + (bv.byteOffset || 0) + (a.byteOffset || 0);
    const out = [];
    for (let k = 0; k < a.count; k++) {
      const v = [];
      for (let c = 0; c < comps; c++) v.push(b.readFloatLE(off + (k * comps + c) * 4));
      out.push(comps == 1 ? v[0] : v);
    }
    return out;
  };
  const anim = j.animations.find((a) => a.name === clipName);
  const tracks = {};
  for (const c of anim.channels) {
    const s = anim.samplers[c.sampler];
    (tracks[c.target.node] ??= {})[c.target.path] = { t: acc(s.input), v: acc(s.output) };
  }
  const len = Math.max(...anim.samplers.map((s) => j.accessors[s.input].max[0]));
  const parent = {};
  j.nodes.forEach((x, i) => (x.children || []).forEach((c) => (parent[c] = i)));
  const samp = (tr, t) => {
    const ts = tr.t;
    if (t <= ts[0]) return tr.v[0];
    if (t >= ts[ts.length - 1]) return tr.v[ts.length - 1];
    let i = 0;
    while (ts[i + 1] < t) i++;
    const f = (t - ts[i]) / (ts[i + 1] - ts[i]);
    const a = tr.v[i], c = tr.v[i + 1];
    return a.length == 4 ? slerp(a, c, f) : a.map((x, k) => x + (c[k] - x) * f);
  };
  const locals = (t) =>
    j.nodes.map((nd, i) => {
      const tr = tracks[i] || {};
      return {
        r: tr.rotation ? samp(tr.rotation, t) : nd.rotation || [0, 0, 0, 1],
        p: tr.translation ? samp(tr.translation, t) : nd.translation || [0, 0, 0],
        s: tr.scale ? samp(tr.scale, t) : nd.scale || [1, 1, 1],
      };
    });
  const globals = (L) => {
    const G = [];
    const g = (i) => {
      if (G[i]) return G[i];
      const l = L[i];
      if (parent[i] === undefined) return (G[i] = { r: l.r, p: l.p, s: l.s });
      const P = g(parent[i]);
      return (G[i] = {
        r: qmul(P.r, l.r),
        p: add(P.p, qrot(P.r, l.p.map((x, k) => x * P.s[k]))),
        s: l.s.map((x, k) => x * P.s[k]),
      });
    };
    L.forEach((_, i) => g(i));
    return G;
  };
  const id = (nm) => names.indexOf('mixamorig:' + nm);
  return { len, locals, globals, id, parent };
}

export const qmul = (a, b) => [
  a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
  a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
  a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
  a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
];
export const qinv = (q) => [-q[0], -q[1], -q[2], q[3]];
export const qrot = (q, v) => qmul(qmul(q, [...v, 0]), qinv(q)).slice(0, 3);
export const axq = (ax, r) => {
  const s = Math.sin(r / 2);
  return [ax[0] * s, ax[1] * s, ax[2] * s, Math.cos(r / 2)];
};
export const add = (a, c) => a.map((x, k) => x + c[k]);
export const sub = (a, c) => a.map((x, k) => x - c[k]);
export const mul = (a, s) => a.map((x) => x * s);
export const dot = (a, c) => a.reduce((s, x, k) => s + x * c[k], 0);
export const cross = (a, c) => [a[1] * c[2] - a[2] * c[1], a[2] * c[0] - a[0] * c[2], a[0] * c[1] - a[1] * c[0]];
export const norm = (a) => Math.hypot(...a);
export const unit = (a) => mul(a, 1 / norm(a));
export function fromTo(u, v) {
  u = unit(u);
  v = unit(v);
  const d = dot(u, v);
  if (d > 0.999999) return [0, 0, 0, 1];
  const q = [...cross(u, v), 1 + d];
  return mul(q, 1 / norm(q));
}
export function slerp(a, c, f) {
  let d = dot(a, c);
  if (d < 0) {
    c = mul(c, -1);
    d = -d;
  }
  if (d > 0.9995) return unit(a.map((x, k) => x + (c[k] - x) * f));
  const th = Math.acos(d), s = Math.sin(th);
  return a.map((x, k) => (Math.sin((1 - f) * th) * x + Math.sin(f * th) * c[k]) / s);
}
export const angle = (u, v) => (Math.acos(Math.max(-1, Math.min(1, dot(unit(u), unit(v))))) * 180) / Math.PI;
export const smooth = (a, c, x) => {
  const t = Math.max(0, Math.min(1, (x - a) / (c - a)));
  return t * t * (3 - 2 * t);
};
// Повороты XYZ движка: q = qz · qy · qx.
export const eulerQuat = (deg) => {
  const [x, y, z] = deg.map((d) => (d * Math.PI) / 180);
  return qmul(axq([0, 0, 1], z), qmul(axq([0, 1, 0], y), axq([1, 0, 0], x)));
};
export const toEuler = (q) => {
  const [x, y, z, w] = q;
  const m20 = 2 * (x * z - y * w), m21 = 2 * (y * z + x * w), m22 = 1 - 2 * (x * x + y * y);
  const m10 = 2 * (x * y + z * w), m00 = 1 - 2 * (y * y + z * z);
  return [Math.atan2(m21, m22), Math.asin(-Math.max(-1, Math.min(1, m20))), Math.atan2(m10, m00)].map((r) => (r * 180) / Math.PI);
};
// Земля склона в осях модели: подъём RISE на длине RUN от Z0, дальше ровный верх. По умолчанию — крутой склон
// клипа «Основа_подъёма_крутой_склон» (build.mjs); setGround меняет его, например на склон исходного клипа.
export let Z0 = 0.26, RUN = 0.8735, RISE = 0.8137, SL = RISE / RUN;
export function setGround(z0, run, rise) { Z0 = z0; RUN = run; RISE = rise; SL = rise / run; }
// Изломы у подножия и у кромки скруглены на ROUND, чтобы наклон под стопой менялся плавно.
const ROUND = 0.03;
const soft = (x) => (x / ROUND > 30 ? x : ROUND * Math.log1p(Math.exp(x / ROUND)));
const sig = (x) => 1 / (1 + Math.exp(-x / ROUND));
export const ground = (z) => SL * (soft(z - Z0) - soft(z - Z0 - RUN));
const rise = (z) => SL * (sig(z - Z0) - sig(z - Z0 - RUN));
export const onSlope = (z) => z > Z0 && z < Z0 + RUN;
export const tangent = (z) => unit([0, rise(z), 1]);
export const normal = (z) => unit([0, 1, -rise(z)]);
