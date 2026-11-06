// Клип «Основа_подъёма_крутой_склон» для «Ямы»: подъём по склону, нарисованному на картинке ямы.
// Исходный клип «Основа_подъёма_длинный_склон» рассчитан на длинный пологий склон (1.7 вперёд, подъём 1.0686,
// 32°) — на картинке зомби начинал карабкаться на плоском дне. Здесь из него вырезается один круг ползания
// (склейка без шва), путь по склону сжимается вперёд до RUN при той же высоте, руки и ноги заново ставятся
// на сжатый склон, корпус наклоняется под него. Ходьба до склона и после него не меняется.
// node build.mjs [glb]   — дописывает клип в модель, убирает исходный длинный, печатает числа для code.lua.
import fs from 'fs';
import { loadRig, qmul, qinv, qrot, axq, add, sub, mul, dot, norm, unit, fromTo, slerp, smooth, ground, Z0, RUN, RISE } from './rig.mjs';

const FILE = process.argv[2] ?? new URL('../../../../../games/pit/models/zombie.glb', import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const SOURCE = 'Основа_подъёма_длинный_склон';
const TARGET = 'Основа_подъёма_крутой_склон';
const SOURCE_RUN = 1.7;
const SOURCE_RISE = 0.44 + 0.44 / 0.7;
const CYCLE = 1.42; // круг ползания: позы в A и A + CYCLE совпадают до 8° на любой кости
const SPLICE = 3.74;
const BLEND = 0.15; // полуширина склейки, с
const FPS = 60;
const KNEE = 0.035; // сустав колена над землёй; ниже — колено уходит в склон, и его отводят в сторону

const R = loadRig(FILE, SOURCE);
const { locals, globals, id, parent } = R;
const HIPS = id('Hips');
const shift = sub(locals(SPLICE + CYCLE)[HIPS].p, locals(SPLICE)[HIPS].p); // сдвиг таза за круг, вдоль склона
const spliceRun = SOURCE_RUN - shift[2];
const spliceRise = SOURCE_RISE - shift[1];
if (Math.abs(spliceRise - RISE) > 1e-3) throw new Error(`высота склона после склейки ${spliceRise}, в rig.mjs ${RISE}`);
const length = R.len - CYCLE;
const squeeze = RUN / spliceRun;

// Поза склеенного клипа: до склейки — исходная, после — на круг позже и на круг ниже по склону.
function spliced(t) {
  const early = locals(t);
  if (t < SPLICE - BLEND) return early;
  const late = locals(t + CYCLE);
  late[HIPS] = { ...late[HIPS], p: sub(late[HIPS].p, shift) };
  if (t > SPLICE + BLEND) return late;
  const w = smooth(SPLICE - BLEND, SPLICE + BLEND, t);
  return early.map((l, i) => ({ r: slerp(l.r, late[i].r, w), p: l.p.map((x, k) => x + (late[i].p[k] - x) * w), s: l.s }));
}
// Точка склеенного клипа → точка на крутом склоне: высота та же, путь по склону короче в squeeze раз.
const squeezeZ = (z) => (z <= Z0 ? z : z >= Z0 + spliceRun ? z - spliceRun + RUN : Z0 + (z - Z0) * squeeze);
const place = (p) => [p[0], p[1], squeezeZ(p[2])];
const onRamp = (z) => z > Z0 && z < Z0 + spliceRun;
const bend = (v, z) => (onRamp(z) ? [v[0], v[1], v[2] * squeeze] : v);

function setGlobalRotation(L, i, r) {
  const G = globals(L);
  L[i] = { ...L[i], r: unit(qmul(qinv(G[parent[i]].r), r)) };
}
function aim(L, i, child, dir) {
  const G = globals(L);
  setGlobalRotation(L, i, qmul(fromTo(sub(G[child].p, G[i].p), dir), G[i].r));
}
// Две кости от root до end: end — в target, сгиб — в сторону pole, повёрнутую на spread градусов вокруг
// линии root → target (плюс — наружу от середины тела).
function reach(L, root, mid, end, target, pole, spread = 0) {
  const G = globals(L);
  const a = G[root].p;
  const l1 = norm(sub(G[mid].p, a)), l2 = norm(sub(G[end].p, G[mid].p));
  const d = sub(target, a);
  const dl = Math.min(norm(d), (l1 + l2) * 0.9995);
  const dir = unit(d);
  const cosA = Math.max(-1, Math.min(1, (l1 * l1 + dl * dl - l2 * l2) / (2 * l1 * dl)));
  let side = sub(pole, a);
  side = unit(sub(side, mul(dir, dot(side, dir))));
  if (spread) side = qrot(axq(dir, ((a[0] > 0 ? -1 : 1) * spread * Math.PI) / 180), side);
  const knee = add(a, add(mul(dir, l1 * cosA), mul(side, l1 * Math.sqrt(1 - cosA * cosA))));
  aim(L, root, mid, sub(knee, a));
  aim(L, mid, end, sub(add(a, mul(dir, dl)), globals(L)[mid].p));
}
// Повернуть кость так, как сжатие склона поворачивает её направление к ребёнку (кисть, стопа, пальцы).
function follow(L, G0, i, child) {
  const v = sub(G0[child].p, G0[i].p);
  setGlobalRotation(L, i, qmul(fromTo(v, bend(v, G0[i].p[2])), G0[i].r));
}

const frames = Math.round(length * FPS);
// Поза кадра; spread — отвод колен в стороны по ногам.
function pose(f, spread) {
  const t = Math.min(length, f / FPS);
  const L = spliced(t);
  const G0 = globals(L);
  const P = (n) => G0[id(n)].p;
  L[HIPS] = { ...L[HIPS], p: place(L[HIPS].p) };
  const chest = P('Neck');
  setGlobalRotation(L, HIPS, qmul(fromTo(sub(chest, P('Hips')), sub(place(chest), place(P('Hips')))), G0[HIPS].r));
  for (const s of ['Left', 'Right']) {
    reach(L, id(s + 'Arm'), id(s + 'ForeArm'), id(s + 'Hand'), place(P(s + 'Hand')), place(P(s + 'ForeArm')));
    follow(L, G0, id(s + 'Hand'), id(s + 'HandMiddle1'));
    reach(L, id(s + 'UpLeg'), id(s + 'Leg'), id(s + 'Foot'), place(P(s + 'Foot')), place(P(s + 'Leg')), spread[s]);
    follow(L, G0, id(s + 'Foot'), id(s + 'ToeBase'));
    follow(L, G0, id(s + 'ToeBase'), id(s + 'Toe_End'));
  }
  return { t, L };
}
const kneeGap = (L, s) => { const p = globals(L)[id(s + 'Leg')].p; return p[1] - ground(p[2]); };
// Сколько надо отвести колено, чтобы оно не ушло в склон; потом сгладить по времени, чтобы колени не дёргались.
const need = { Left: [], Right: [] };
for (let f = 0; f <= frames; f++) {
  for (const s of ['Left', 'Right']) {
    let spread = 0;
    while (spread < 80 && kneeGap(pose(f, { Left: 0, Right: 0, [s]: spread }).L, s) < KNEE) spread += 2;
    need[s].push(spread);
  }
}
const spreadOf = {};
for (const s of ['Left', 'Right']) {
  const r = need[s];
  const wide = r.map((_, i) => Math.max(...r.slice(Math.max(0, i - 6), i + 7)));
  spreadOf[s] = wide.map((_, i) => { const part = wide.slice(Math.max(0, i - 8), i + 9); return part.reduce((a, b) => a + b, 0) / part.length; });
}
const poses = [];
for (let f = 0; f <= frames; f++) poses.push(pose(f, { Left: spreadOf.Left[f], Right: spreadOf.Right[f] }));

// Запись: клип с ключом на каждый кадр для всех каналов исходного клипа, исходный длинный клип убирается,
// данные, на которые больше никто не ссылается, выбрасываются из буфера.
const raw = fs.readFileSync(FILE);
const jsonLength = raw.readUInt32LE(12);
const doc = JSON.parse(raw.slice(20, 20 + jsonLength));
const bin = raw.slice(28 + jsonLength);
const viewOf = (i) => { const v = doc.bufferViews[i]; return bin.slice(v.byteOffset ?? 0, (v.byteOffset ?? 0) + v.byteLength); };
const source = doc.animations.find((a) => a.name === SOURCE);
const floats = (values, type) => {
  const flat = values.flat();
  const buffer = Buffer.alloc(flat.length * 4);
  flat.forEach((v, k) => buffer.writeFloatLE(v, k * 4));
  doc.bufferViews.push({ buffer: 0, byteOffset: -1, byteLength: buffer.length, _data: buffer });
  const width = { SCALAR: 1, VEC3: 3, VEC4: 4 }[type];
  const accessor = { bufferView: doc.bufferViews.length - 1, componentType: 5126, count: values.length, type };
  if (type === 'SCALAR') { accessor.min = [Math.min(...flat)]; accessor.max = [Math.max(...flat)]; }
  else { accessor.min = [...Array(width)].map((_, c) => Math.min(...values.map((v) => v[c]))); accessor.max = [...Array(width)].map((_, c) => Math.max(...values.map((v) => v[c]))); }
  doc.accessors.push(accessor);
  return doc.accessors.length - 1;
};
const time = floats(poses.map((p) => p.t), 'SCALAR');
const field = { rotation: 'r', translation: 'p', scale: 's' };
const target = { name: TARGET, channels: [], samplers: [] };
for (const channel of source.channels) {
  const node = channel.target.node;
  const type = channel.target.path === 'rotation' ? 'VEC4' : 'VEC3';
  let previous = null;
  const values = poses.map(({ L }) => {
    let v = L[node][field[channel.target.path]];
    if (type === 'VEC4' && previous && dot(v, previous) < 0) v = mul(v, -1);
    previous = v;
    return v;
  });
  target.samplers.push({ input: time, output: floats(values, type), interpolation: 'LINEAR' });
  target.channels.push({ sampler: target.samplers.length - 1, target: { node, path: channel.target.path } });
}
doc.animations = doc.animations.filter((a) => a !== source && a.name !== TARGET);
doc.animations.push(target);

const usedAccessors = new Set();
for (const mesh of doc.meshes ?? []) for (const prim of mesh.primitives) {
  Object.values(prim.attributes).forEach((a) => usedAccessors.add(a));
  if (prim.indices !== undefined) usedAccessors.add(prim.indices);
  for (const morph of prim.targets ?? []) Object.values(morph).forEach((a) => usedAccessors.add(a));
}
for (const skin of doc.skins ?? []) if (skin.inverseBindMatrices !== undefined) usedAccessors.add(skin.inverseBindMatrices);
for (const a of doc.animations) for (const s of a.samplers) { usedAccessors.add(s.input); usedAccessors.add(s.output); }
const accessorMap = new Map();
const accessors = [];
[...usedAccessors].sort((a, b) => a - b).forEach((i) => { accessorMap.set(i, accessors.length); accessors.push(doc.accessors[i]); });
const usedViews = new Set(accessors.map((a) => a.bufferView).filter((v) => v !== undefined));
for (const image of doc.images ?? []) if (image.bufferView !== undefined) usedViews.add(image.bufferView);
const viewMap = new Map();
const chunks = [];
let offset = 0;
const bufferViews = [];
[...usedViews].sort((a, b) => a - b).forEach((i) => {
  const v = doc.bufferViews[i];
  const data = v._data ?? viewOf(i);
  const { _data, ...rest } = v;
  viewMap.set(i, bufferViews.length);
  bufferViews.push({ ...rest, byteOffset: offset, byteLength: data.length });
  const padded = Buffer.alloc(Math.ceil(data.length / 4) * 4);
  data.copy(padded);
  chunks.push(padded);
  offset += padded.length;
});
for (const a of accessors) if (a.bufferView !== undefined) a.bufferView = viewMap.get(a.bufferView);
for (const image of doc.images ?? []) if (image.bufferView !== undefined) image.bufferView = viewMap.get(image.bufferView);
for (const mesh of doc.meshes ?? []) for (const prim of mesh.primitives) {
  for (const k of Object.keys(prim.attributes)) prim.attributes[k] = accessorMap.get(prim.attributes[k]);
  if (prim.indices !== undefined) prim.indices = accessorMap.get(prim.indices);
  for (const morph of prim.targets ?? []) for (const k of Object.keys(morph)) morph[k] = accessorMap.get(morph[k]);
}
for (const skin of doc.skins ?? []) if (skin.inverseBindMatrices !== undefined) skin.inverseBindMatrices = accessorMap.get(skin.inverseBindMatrices);
for (const a of doc.animations) for (const s of a.samplers) { s.input = accessorMap.get(s.input); s.output = accessorMap.get(s.output); }
doc.accessors = accessors;
doc.bufferViews = bufferViews;
doc.buffers = [{ byteLength: offset }];
let json = Buffer.from(JSON.stringify(doc));
json = Buffer.concat([json, Buffer.alloc((4 - (json.length % 4)) % 4, 0x20)]);
const body = Buffer.concat(chunks);
const header = Buffer.alloc(12);
header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4);
header.writeUInt32LE(12 + 8 + json.length + 8 + body.length, 8);
const chunkHeader = (len, type) => { const b = Buffer.alloc(8); b.writeUInt32LE(len, 0); b.writeUInt32LE(type, 4); return b; };
fs.writeFileSync(FILE, Buffer.concat([header, chunkHeader(json.length, 0x4e4f534a), json, chunkHeader(body.length, 0x004e4942), body]));

const first = poses[0].L[HIPS].p, last = poses[poses.length - 1].L[HIPS].p;
console.log('отвод колен, наибольший', Math.max(...need.Left), Math.max(...need.Right));
console.log(JSON.stringify({ clip: TARGET, seconds: +length.toFixed(4), keys: poses.length, squeeze: +squeeze.toFixed(4),
  rampRun: RUN, rampRise: RISE, rootTravel: +(last[2] - first[2]).toFixed(4), rootRise: +(last[1] - first[1]).toFixed(4), bytes: 12 + 16 + json.length + body.length }));
