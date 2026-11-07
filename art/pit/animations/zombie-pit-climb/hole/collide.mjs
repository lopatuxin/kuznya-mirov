// Замер провалов тела в землю у клипа вылезания: каждая точка сетки зомби в каждом кадре проверяется против земли
// и круглой норы в осях модели (рост модели ≈ 1, в игре рост = HEIGHT клеток). Точка вне круга ниже земли — в земле
// или в стенке (меряется вглубь стенки); точка в круге ниже дна — в дне. Печатает худшие места по времени и кость, что ведёт точку.
// node collide.mjs [glb] [клип] [шаг, с]
import fs from 'fs';
import { loadRig } from '../steep-slope/rig.mjs';

const here = (p) => new URL(p, import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const FILE = process.argv[2] || here('../../../../../games/pit/models/zombie.glb');
const CLIP = process.argv[3] || 'Основа_вылезания_из_ямы';
const STEP = +(process.argv[4] ?? 0.1);
// Нора в осях модели: радиус и глубина в клетках игры, делённые на рост зомби; середина — на радиус за кромкой.
const HEIGHT = +(process.env.HEIGHT ?? 4.0467);
const RADIUS = 1.45 / HEIGHT, DEPTH = +(process.env.DEPTH ?? 6.2) / HEIGHT, RIM = 0.24;
const CZ = RIM - RADIUS;
const TOLERANCE = 0.006;

const raw = fs.readFileSync(FILE);
const n = raw.readUInt32LE(12);
const doc = JSON.parse(raw.slice(20, 20 + n));
const bin = raw.slice(28 + n);
function read(index) {
  const a = doc.accessors[index], v = doc.bufferViews[a.bufferView];
  const width = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 }[a.type];
  const size = { 5126: 4, 5121: 1, 5123: 2 }[a.componentType];
  const stride = v.byteStride ?? width * size;
  const at = (v.byteOffset ?? 0) + (a.byteOffset ?? 0);
  const get = { 5126: (o) => bin.readFloatLE(o), 5121: (o) => bin.readUInt8(o), 5123: (o) => bin.readUInt16LE(o) }[a.componentType];
  return [...Array(a.count)].map((_, k) => [...Array(width)].map((_, c) => get(at + k * stride + c * size)));
}
const prim = doc.meshes[0].primitives[0];
const positions = read(prim.attributes.POSITION);
const joints = read(prim.attributes.JOINTS_0);
const weights = read(prim.attributes.WEIGHTS_0);
const skin = doc.skins[0];
const ibm = read(skin.inverseBindMatrices);
const R = loadRig(FILE, CLIP);
const names = doc.nodes.map((x) => x.name);

const mat = (g) => {
  const [x, y, z, w] = g.r;
  const m = [1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w), 0,
    2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w), 0,
    2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y), 0, 0, 0, 0, 1];
  for (let c = 0; c < 3; c++) for (let r = 0; r < 3; r++) m[c * 4 + r] *= g.s[c];
  m[12] = g.p[0]; m[13] = g.p[1]; m[14] = g.p[2];
  return m;
};
const mul = (a, b) => { const o = Array(16).fill(0); for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k]; return o; };
const apply = (m, p) => [0, 1, 2].map((r) => m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]);

const rows = [];
let worst = { depth: 0 };
for (let t = 0; t <= R.len + 1e-6; t += STEP) {
  const G = R.globals(R.locals(t));
  const skinM = skin.joints.map((node, k) => mul(mat(G[node]), ibm[k]));
  const byBone = {};
  for (let v = 0; v < positions.length; v++) {
    const p = [0, 0, 0];
    for (let k = 0; k < 4; k++) {
      if (!weights[v][k]) continue;
      const q = apply(skinM[joints[v][k]], positions[v]);
      for (let c = 0; c < 3; c++) p[c] += q[c] * weights[v][k];
    }
    const off = Math.hypot(p[0], p[2] - CZ) - RADIUS;
    const depth = off < 0 ? -DEPTH - p[1] : Math.min(-p[1], off); // в стенке — насколько вглубь стенки
    if (depth <= TOLERANCE) continue;
    const lead = skin.joints[joints[v][weights[v].indexOf(Math.max(...weights[v]))]];
    const bone = names[lead].replace('mixamorig:', '');
    byBone[bone] = Math.max(byBone[bone] ?? 0, depth);
    if (depth > worst.depth) worst = { depth, t, bone };
  }
  const list = Object.entries(byBone).sort((a, b) => b[1] - a[1]).slice(0, 5).map(([b, d]) => `${b} ${(d * HEIGHT * 100).toFixed(0)}см`);
  if (list.length) rows.push(`${t.toFixed(2)}  ${list.join(', ')}`);
}
console.log(rows.join('\n'));
console.log('худшее', JSON.stringify({ ...worst, worldCm: +(worst.depth * HEIGHT * 100).toFixed(0) }));
