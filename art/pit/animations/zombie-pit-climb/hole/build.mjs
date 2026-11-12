// Клип «Основа_вылезания_из_ямы»: зомби поднимается по стенке норы из глубины (сначала из темноты показывается
// макушка, руки цепляются за стенку ниже головы), закидывает руки на кромку, подтягивается, переваливается грудью,
// ползёт на животе, пока ноги не лягут на землю, подтягивает колени и встаёт в первую позу ходьбы «ходьба_зомби_кино».
// Порядок фаз — одобренные кадры нарисованного вылезания (source/zombie-07-climb-sheet.png) и одобренный
// референс подъёма к краю (reference.json, emergenceSource). Тело ставится по ключам таза и наклона,
// руки и ноги — обратной кинематикой к опорам: кисть на земле не скользит, пока на неё опираются.
// Нора та же, что в игре (art/pit/ground/build_ground_hole.mjs): круг радиусом 1,45 клетки при росте зомби HEIGHT;
// локти и колени в каждом кадре отводятся туда, где рука или нога не входит в землю и в стенку норы.
// Оси модели: X — влево, Y — вверх, Z — вперёд; земля — Y = 0, кромка норы впереди — Z = RIM.
// node build.mjs [glb] [motions.json] [--north] — заменяет выбранный клип, печатает замеры опор и провалов.
import fs from 'fs';
import { loadRig, qmul, qinv, axq, add, sub, mul, dot, norm, unit, fromTo, slerp, smooth, eulerQuat, qrot, cross, toEuler } from '../steep-slope/rig.mjs';

const here = (p) => new URL(p, import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const args = process.argv.slice(2);
const north = args.at(-1) === '--north';
if (north) args.pop();
const FILE = args[0] || here('../../../../../games/pit/models/zombie.glb');
const MOTIONS = args[1] || here('../../../../../games/pit/motions.json');
const BASE_CLIP = 'Основа_шага';
const BASE_MOTION = 'ходьба_зомби_кино';
const TARGET = north ? 'Основа_вылезания_к_забору' : 'Основа_вылезания_из_ямы';
const FPS = 60;
const RIM = 0.24;
const LENGTH = 8.5;
const NORTH_RIM_DELAY = 2.1;
const BLEND_FROM = 8.2; // с этого места поза сходится к первой позе ходьбы
const END_Z = 0.7; // где встаёт таз
// Нора в осях модели: рост зомби в игре HEIGHT клеток, радиус норы 1,45 клетки, глубина 6,2 клетки.
const HEIGHT = north ? 2.1 : 4.0467;
const HOLE_R = 1.45 / HEIGHT, HOLE_DEPTH = 6.2 / HEIGHT, CZ = RIM - HOLE_R;

// Ключи: [время, значение]; между ключами — монотонная кубическая кривая по каждой оси:
// без выбросов, а одинаковые соседние ключи дают полную неподвижность (опора не плывёт).
function track(keys) {
  const t = keys.map((k) => k[0]);
  const v = keys.map((k) => (Array.isArray(k[1]) ? k[1] : [k[1]]));
  const dims = v[0].length;
  const slopes = [...Array(dims)].map((_, d) => {
    const n = t.length;
    const del = [...Array(n - 1)].map((_, i) => (v[i + 1][d] - v[i][d]) / (t[i + 1] - t[i]));
    const m = [...Array(n)].map((_, i) => {
      if (i === 0) return del[0] ?? 0;
      if (i === n - 1) return del[n - 2];
      if (del[i - 1] * del[i] <= 0) return 0;
      const w1 = 2 * (t[i + 1] - t[i]) + (t[i] - t[i - 1]), w2 = (t[i + 1] - t[i]) + 2 * (t[i] - t[i - 1]);
      return (w1 + w2) / (w1 / del[i - 1] + w2 / del[i]);
    });
    if (n > 1) { if (del[0] === 0) m[0] = 0; if (del[n - 2] === 0) m[n - 1] = 0; }
    return m;
  });
  return (x) => {
    if (x <= t[0]) return dims === 1 ? v[0][0] : v[0];
    const last = t.length - 1;
    if (x >= t[last]) return dims === 1 ? v[last][0] : v[last];
    let i = 0;
    while (t[i + 1] < x) i++;
    const h = t[i + 1] - t[i], s = (x - t[i]) / h;
    const h00 = 2 * s ** 3 - 3 * s ** 2 + 1, h10 = s ** 3 - 2 * s ** 2 + s, h01 = -2 * s ** 3 + 3 * s ** 2, h11 = s ** 3 - s ** 2;
    const out = [...Array(dims)].map((_, d) => h00 * v[i][d] + h10 * h * slopes[d][i] + h01 * v[i + 1][d] + h11 * h * slopes[d][i + 1]);
    return dims === 1 ? out[0] : out;
  };
}
const G = 0.045; // сустав кисти над землёй, когда ладонь лежит: пальцы не уходят в землю
const EDGE = 0.22; // кисть, закинутая на кромку: запястье сразу за краем норы

// На дальнем краю камера смотрит вдоль стенки. Отдельные опоры дают начать ниже и
// подняться до кромки перехватами, пока ноги отталкиваются от стенки.
function wallSteps(x, onWall, offWall, holds) {
  const keys = [];
  for (const [index, [plant, release, y]] of holds.entries()) {
    if (index > 0) {
      const [, previousRelease, previousY] = holds[index - 1];
      keys.push([(previousRelease + plant) / 2, [x, (previousY + y) / 2, offWall]]);
    }
    const contact = [x, y, onWall];
    keys.push([plant, contact], [release, contact]);
  }
  return keys;
}

const northHips = [
  [0, [0, -2.22, 0.012]], [0.16, [-0.012, -2.14, 0.016]], [0.48, [0.012, -1.88, 0.023]],
  [0.9, [-0.012, -1.54, 0.026]], [1.25, [0.01, -1.27, 0.028]], [1.59, [-0.01, -1, 0.035]],
  [1.9, [0.01, -0.74, 0.037]], [2.1, [0, -0.57, 0.04]], [2.3, [0, -0.47, 0.04]],
];
const northHands = {
  Left: wallSteps(0.16, 0.18, 0.13, [
    [0, 0.10, -2.12], [0.28, 0.50, -1.68], [0.68, 0.91, -1.5],
    [1.08, 1.30, -1.03], [1.47, 1.69, -0.73], [1.86, 2.05, -0.4],
  ]),
  Right: wallSteps(-0.16, 0.18, 0.13, [
    [0, 0.29, -2.03], [0.47, 0.71, -1.58], [0.88, 1.10, -1.29],
    [1.27, 1.49, -0.88], [1.66, 1.89, -0.58], [2.08, 2.18, -0.22],
  ]),
};
const northFeet = {
  Left: wallSteps(0.08, 0.1, 0.065, [
    [0, 0.12, -2.54], [0.29, 0.54, -2.25], [0.70, 0.89, -1.94],
    [1.07, 1.31, -1.62], [1.48, 1.71, -1.31], [1.87, 2.12, -0.98],
  ]),
  Right: wallSteps(-0.08, 0.1, 0.065, [
    [0, 0.32, -2.44], [0.49, 0.73, -2.09], [0.90, 1.14, -1.77],
    [1.30, 1.53, -1.45], [1.69, 1.92, -1.13], [2.07, 2.4, -0.82],
  ]),
};

const hips = track([
  ...(north ? northHips : [[0, [0, -0.98, 0]], [0.7, [0, -0.8, 0.01]], [1.4, [0, -0.6, 0.02]]]),
  [2.6, [0, -0.42, 0.04]], [2.9, [0, -0.38, 0.03]],
  [3.9, [0, -0.11, 0.06]], [4.5, [0, 0.07, 0.16]], [5.1, [0, 0.1, 0.28]], [5.7, [0, 0.11, 0.4]], [6.3, [0, 0.13, 0.52]],
  [6.9, [0, 0.24, 0.56]], [7.5, [0, 0.38, 0.66]], [8.5, [0, 0.493, END_Z]],
]);
// Наклон корпуса вперёд, сгорбленность спины и подъём головы, градусы.
const lean = track([
  ...(north ? [[0, 7], [1.4, 7], [2.0, 3]] : [[0, -3], [1.4, -4], [2.0, -8]]),
  [2.6, -2], [2.9, 5], [3.9, 30], [4.5, 55], [5.1, 75], [6.3, 82], [6.9, 72], [7.5, 40], [8.5, 0],
]);
const hunch = track([[0, 0], [2.9, 5], [3.9, 12], [6.9, 12], [7.5, 8], [8.5, 0]]);
const headUp = track([[0, 15], [1.4, 25], [2.6, 20], [2.9, 10], [3.9, 25], [4.5, 45], [5.1, 60], [6.3, 60], [6.9, 50], [7.5, 25], [8.5, 0]]);
// Лёгкое покачивание корпуса за опорной рукой: зомби переваливается.
const roll = track([[0, 0], [0.35, 4], [0.75, -4], [1.15, 3], [2.7, 6], [3.0, -4], [3.8, 4], [4.3, -5], [4.7, 5], [5.25, -5], [5.8, 5], [6.35, -5], [6.9, 4], [7.4, 0]]);

// Кисти: в глубине — перехват по стенке ниже головы, потом закинуты на кромку, потом подтягивания на животе и
// упор на четвереньках.
const hands = {
  Left: track([
    ...(north ? [...northHands.Left, [2.22, [0.16, -0.24, 0.123]]] : [
      [0, [0.16, -0.82, 0.15]], [0.4, [0.16, -0.82, 0.15]], [0.7, [0.17, -0.6, 0.15]], [1.3, [0.17, -0.6, 0.15]], [1.9, [0.21, -0.4, 0.12]],
    ]),
    [2.4, [0.16, 0.07, 0.2]], [2.7, [0.15, G, EDGE]], [4.15, [0.15, G, EDGE]], [4.3, [0.24, 0.07, 0.35]], [4.45, [0.17, G, 0.47]],
    [5.2, [0.17, G, 0.47]], [5.35, [0.23, 0.07, 0.56]], [5.5, [0.17, G, 0.65]], [6.3, [0.17, G, 0.65]], [6.45, [0.22, 0.07, 0.74]],
    [6.6, [0.17, G, 0.82]], [7.3, [0.17, G, 0.82]], [7.9, [0.117, 0.457, 0.679]],
  ]),
  Right: track([
    ...(north ? [...northHands.Right, [2.37, [-0.16, -0.14, 0.126]]] : [
      [0, [-0.16, -0.7, 0.15]], [0.2, [-0.16, -0.7, 0.15]], [0.55, [-0.17, -0.5, 0.15]], [1.1, [-0.17, -0.5, 0.15]], [2.1, [-0.21, -0.32, 0.12]],
    ]),
    [2.6, [-0.16, 0.08, 0.2]], [2.9, [-0.15, G, EDGE]], [4.4, [-0.15, G, EDGE]], [4.55, [-0.24, 0.07, 0.38]], [4.7, [-0.18, G, 0.5]],
    [5.75, [-0.18, G, 0.5]], [5.9, [-0.23, 0.07, 0.62]], [6.05, [-0.17, G, 0.72]], [6.85, [-0.17, G, 0.72]], [7.0, [-0.22, 0.07, 0.8]],
    [7.15, [-0.17, G, 0.88]], [7.3, [-0.17, G, 0.88]], [7.9, [-0.117, 0.461, 0.778]],
  ]),
};
// Насколько ладонь лежит на стенке норы пальцами вверх (1), пока зомби перехватывается по ней.
const wallPalm = {
  Left: track([[0, 1], [2.0, 1], [2.3, 0]]),
  Right: track([[0, 1], [2.2, 1], [2.5, 0]]),
};
// Насколько кисть лежит ладонью на земле (1) или тянется по линии предплечья (0).
const palm = {
  Left: track([[0, 0], [2.3, 0], [2.7, 1], [4.15, 1], [4.25, 0.85], [4.45, 1], [5.2, 1], [5.3, 0.85], [5.5, 1], [6.3, 1], [6.4, 0.85], [6.6, 1], [7.3, 1], [7.95, 0]]),
  Right: track([[0, 0], [2.5, 0], [2.9, 1], [4.4, 1], [4.5, 0.85], [4.7, 1], [5.75, 1], [5.85, 0.85], [6.05, 1], [6.85, 1], [6.95, 0.85], [7.15, 1], [7.3, 1], [7.95, 0]]),
};
// Стопы: шаги по стенке норы, потом ноги волочатся за тазом, пока он ползёт на животе, колени подтягиваются,
// выпад и стойка.
const feet = {
  Left: track([
    ...(north ? northFeet.Left : [
      [0, [0.08, -1.38, 0.05]], [0.3, [0.08, -1.38, 0.05]], [0.6, [0.08, -1.15, 0.05]], [1.0, [0.08, -1.15, 0.05]], [1.4, [0.08, -0.92, 0.05]],
      [2.0, [0.08, -0.92, 0.05]],
    ]),
    [2.35, [0.08, -0.78, 0.05]], [2.85, [0.08, -0.78, 0.05]], [3.25, [0.08, -0.68, 0.05]], [3.55, [0.08, -0.68, 0.05]],
    [3.8, [0.08, -0.5, 0.05]], [4.2, [0.08, -0.5, 0.05]], [4.6, [0.08, -0.33, 0.0]], [5.1, [0.09, -0.32, 0.05]], [5.7, [0.09, -0.22, 0.1]],
    [6.1, [0.09, -0.02, 0.18]], [6.5, [0.09, 0.07, 0.26]], [6.9, [0.09, 0.07, 0.34]], [7.75, [0.09, 0.07, 0.34]], [8.0, [0.08, 0.18, 0.62]], [8.25, [0.073, 0.074, 0.81]],
    [8.5, [0.073, 0.074, 0.81]],
  ]),
  Right: track([
    ...(north ? northFeet.Right : [
      [0, [-0.08, -1.25, 0.05]], [0.45, [-0.08, -1.25, 0.05]], [0.8, [-0.08, -1.02, 0.05]], [1.3, [-0.08, -1.02, 0.05]], [1.65, [-0.08, -0.85, 0.05]],
    ]),
    [2.8, [-0.08, -0.85, 0.05]], [3.35, [-0.08, -0.58, 0.05]], [3.8, [-0.08, -0.58, 0.05]], [4.05, [-0.08, -0.38, 0.05]], [4.6, [-0.08, -0.38, 0.05]],
    [5.0, [-0.08, -0.3, 0.03]], [5.5, [-0.09, -0.3, 0.06]], [6.1, [-0.09, -0.18, 0.11]], [6.5, [-0.09, -0.02, 0.16]], [6.75, [-0.09, 0.06, 0.24]], [7.0, [-0.09, 0.07, 0.36]],
    [7.3, [-0.09, 0.07, 0.36]], [7.45, [-0.09, 0.16, 0.52]], [7.7, [-0.073, 0.077, 0.679]], [8.5, [-0.073, 0.077, 0.679]],
  ]),
};
// Куда смотрит локоть от плеча (X — наружу): в стороны, пока перехватывается по стенке; вниз вдоль стенки, пока
// висит на кромке; назад и вверх, когда выжимается над кромкой; назад, пока ползёт и стоит на четвереньках.
const elbows = {
  Left: track([[0, [0.6, -0.1, -0.3]], [2.4, [0.6, -0.1, -0.3]], [2.7, [0.15, -1, -0.6]], [3.8, [0.15, -1, -0.6]], [4.2, [0.25, 0.3, -1]],
    [4.5, [0.3, 0, -1]], [7.3, [0.3, 0, -1]], [7.9, [0.4, -0.3, -0.5]]]),
  Right: track([[0, [0.6, -0.1, -0.3]], [2.6, [0.6, -0.1, -0.3]], [2.9, [0.15, -1, -0.6]], [3.8, [0.15, -1, -0.6]], [4.2, [0.25, 0.3, -1]],
    [4.5, [0.3, 0, -1]], [7.3, [0.3, 0, -1]], [7.9, [0.4, -0.3, -0.5]]]),
};
// Куда смотрит колено от тазобедренного сустава (X — наружу); отвод от стенки добавляется в каждом кадре.
const knees = {
  Left: track([[0, [0.05, -0.3, 1]], [2.85, [0.05, -0.3, 1]], [3.25, [0.05, -0.1, 1]], [4.2, [0.05, -0.1, 1]], [4.6, [0.05, -1, -0.3]], [6.1, [0.05, -1, -0.3]],
    [6.6, [0.05, -0.6, 1]], [7.75, [0.05, -0.6, 1]], [7.85, [0.05, 0.3, 1]], [8.25, [0.03, -0.1, 1]]]),
  Right: track([[0, [0.05, -0.3, 1]], [2.8, [0.05, -0.3, 1]], [3.35, [0.05, -0.1, 1]], [4.6, [0.05, -0.1, 1]], [5.0, [0.05, -1, -0.3]], [6.2, [0.05, -1, -0.3]],
    [6.7, [0.05, -0.6, 1]], [7.3, [0.05, -0.6, 1]], [7.4, [0.05, 0.3, 1]], [7.7, [0.03, -0.1, 1]]]),
};
// Куда смотрят пальцы ноги: носком в стенку, назад, пока волочатся и на коленях, вперёд в стойке.
const toes = {
  Left: track([[0, [0, -0.2, 1]], [4.2, [0, -0.1, 1]], [4.7, [0, -1, -0.3]], [5.3, [0, -0.4, -1]], [7.75, [0, -0.4, -1]], [8.0, [0, -1, 0.1]], [8.25, [0, -0.3, 1]]]),
  Right: track([[0, [0, -0.2, 1]], [4.6, [0, -0.1, 1]], [5.1, [0, -1, -0.3]], [5.6, [0, -0.4, -1]], [7.3, [0, -0.4, -1]], [7.5, [0, -1, 0.1]], [7.7, [0, -0.3, 1]]]),
};

// Основа — первая поза ходьбы с поправками движения «ходьба_зомби_кино»: в ней зомби и заканчивает.
const R = loadRig(FILE, BASE_CLIP);
const { globals, id, parent } = R;
const names = JSON.parse(fs.readFileSync(FILE).slice(20, 20 + fs.readFileSync(FILE).readUInt32LE(12))).nodes.map((n) => n.name);
const HIPS = id('Hips');
const walkBones = JSON.parse(fs.readFileSync(MOTIONS, 'utf8'))[BASE_MOTION]?.bones ?? {};
const base = R.locals(0);
for (const [name, marks] of Object.entries(walkBones)) {
  const i = id(name.replace('mixamorig:', ''));
  const turn = typeof marks[0] === 'number' ? marks : marks[0].turn;
  base[i] = { ...base[i], r: qmul(base[i].r, eulerQuat(turn)) };
}
const baseHipsP = base[HIPS].p;

function setGlobalRotation(L, i, r) {
  const Gs = globals(L);
  L[i] = { ...L[i], r: unit(qmul(qinv(Gs[parent[i]].r), r)) };
}
const turnGlobal = (L, i, q) => setGlobalRotation(L, i, qmul(q, globals(L)[i].r));
function aim(L, i, child, dir) {
  const Gs = globals(L);
  turnGlobal(L, i, fromTo(sub(Gs[child].p, Gs[i].p), dir));
}
// Две кости от root до end: end — в target, сгиб — в сторону точки pole, повёрнутой на spread градусов вокруг
// линии root → target.
function reach(L, root, mid, end, target, pole, spread = 0) {
  const Gs = globals(L);
  const a = Gs[root].p;
  const l1 = norm(sub(Gs[mid].p, a)), l2 = norm(sub(Gs[end].p, Gs[mid].p));
  const d = sub(target, a);
  const dl = Math.min(norm(d), (l1 + l2) * 0.9995);
  const dir = unit(d);
  const cosA = Math.max(-1, Math.min(1, (l1 * l1 + dl * dl - l2 * l2) / (2 * l1 * dl)));
  let side = sub(pole, a);
  side = unit(sub(side, mul(dir, dot(side, dir))));
  if (spread) side = qrot(axq(dir, (spread * Math.PI) / 180), side);
  const joint = add(a, add(mul(dir, l1 * cosA), mul(side, l1 * Math.sqrt(1 - cosA * cosA))));
  aim(L, root, mid, sub(joint, a));
  aim(L, mid, end, sub(add(a, mul(dir, dl)), globals(L)[mid].p));
}

// Насколько шар радиуса r с серединой p вошёл в землю, в стенку норы или в её дно.
function sink(p, r) {
  const off = Math.hypot(p[0], p[2] - CZ) - HOLE_R;
  let gap;
  if (off > 0) gap = p[1] >= 0 ? p[1] : -Math.min(-p[1], off);
  else gap = p[1] >= 0 ? Math.hypot(p[1], off) : -off;
  return Math.max(0, r - gap, r - (p[1] + HOLE_DEPTH));
}
// Провал кости от a до b: шары вдоль неё, наибольший.
const boneSink = (a, b, r) => Math.max(...[0.1, 0.3, 0.5, 0.7, 0.9].map((f) => sink(add(a, mul(sub(b, a), f)), r)));
const LIMBS = {
  Arm: { parts: ['Arm', 'ForeArm', 'Hand'], radius: [0.035, 0.03] },
  Leg: { parts: ['UpLeg', 'Leg', 'Foot'], radius: [0.05, 0.04] },
};
function limbSink(L, s, kind) {
  const Gs = globals(L);
  const [a, b, c] = LIMBS[kind].parts.map((n) => Gs[id(s + n)].p);
  return Math.max(boneSink(a, b, LIMBS[kind].radius[0]), boneSink(b, c, LIMBS[kind].radius[1]));
}

const X = [1, 0, 0], Z = [0, 0, 1];
const rad = (d) => (d * Math.PI) / 180;
const SIDES = ['Left', 'Right'];
// Тело кадра без рук и ног.
function body(t) {
  const L = base.map((l) => ({ ...l }));
  L[HIPS] = { ...L[HIPS], p: hips(t) };
  turnGlobal(L, HIPS, qmul(axq(Z, rad(roll(t))), axq(X, rad(lean(t)))));
  for (const s of ['Spine', 'Spine1', 'Spine2']) turnGlobal(L, id(s), axq(X, rad(hunch(t) / 3)));
  turnGlobal(L, id('Neck'), axq(X, rad(-headUp(t) * 0.4)));
  turnGlobal(L, id('Head'), axq(X, rad(-headUp(t) * 0.6)));
  return L;
}
function arm(L, s, t, spread) {
  const out = s === 'Left' ? 1 : -1;
  const shoulder = globals(L)[id(s + 'Arm')].p;
  const e = elbows[s](t);
  reach(L, id(s + 'Arm'), id(s + 'ForeArm'), id(s + 'Hand'), hands[s](t), add(shoulder, [out * e[0], e[1], e[2]]), out * spread);
  const g2 = globals(L);
  const fore = unit(sub(g2[id(s + 'Hand')].p, g2[id(s + 'ForeArm')].p));
  const flat = unit([out * 0.1, -0.05, 1]);
  const up = unit([out * 0.2, 1, -0.15]);
  const p = palm[s](t), w = wallPalm[s](t) * (1 - p);
  aim(L, id(s + 'Hand'), id(s + 'HandMiddle1'), unit(add(add(mul(fore, 1 - p - w), mul(flat, p)), mul(up, w))));
  // Ладонь — к опоре: на земле вниз, на стенке к стенке; в воздухе — вниз и вперёд.
  const want = unit(add(add(mul([0, -0.5, 1], 1 - p - w), mul([0, -1, 0], p)), mul([0, 0, 1], w)));
  const g3 = globals(L);
  const f = unit(sub(g3[id(s + 'HandMiddle1')].p, g3[id(s + 'Hand')].p));
  const n = palmFacing(g3, s);
  const goal = sub(want, mul(f, dot(want, f)));
  if (norm(goal) > 1e-3) turnGlobal(L, id(s + 'Hand'), axq(f, palmTurn(t) * Math.atan2(dot(cross(n, goal), f), dot(n, goal))));
}
// Доля доворота ладони: вся, пока руки работают; к стойке сходит на нет, и кисти висят, как в ходьбе.
const palmTurn = track([[0, 1], [7.3, 1], [7.9, 0]]);
// Куда смотрит ладонь: перпендикуляр к кисти в ту сторону, куда загибаются пальцы.
function palmFacing(Gs, s) {
  const P = (k) => Gs[id(s + k)].p;
  const f = unit(sub(P('HandMiddle1'), P('Hand')));
  const n = unit(cross(f, unit(sub(P('HandIndex1'), P('HandPinky1')))));
  const c = sub(P('HandMiddle3'), P('HandMiddle1'));
  return dot(sub(c, mul(f, dot(c, f))), n) > 0 ? n : mul(n, -1);
}
// Нога, что лежит на кромке, пока зомби ползёт через край на животе: бедро лежит на углу кромки, голень
// продолжает его, чуть согнутая пятками вверх, колено смотрит вниз — как у человека на животе. Колено сгибается
// только назад: голень, свисающая вниз из-за бедра, что уходит назад, — колено, вывернутое в обратную сторону. Доля по времени: 0 — нога по ключам, 1 — на кромке.
const drape = {
  Left: track([[0, 0], [4.7, 0], [5.0, 1], [6.4, 1], [6.8, 0]]),
  Right: track([[0, 0], [5.0, 0], [5.3, 1], [6.5, 1], [6.9, 0]]),
};
const THIGH = norm(sub(globals(base)[id('LeftLeg')].p, globals(base)[id('LeftUpLeg')].p));
const SHIN = norm(sub(globals(base)[id('LeftFoot')].p, globals(base)[id('LeftLeg')].p));
const DRAPE_BEND = rad(15);
function hanging(hip) {
  const edge = CZ + Math.sqrt(Math.max(0, HOLE_R * HOLE_R - hip[0] * hip[0]));
  const corner = [hip[0], 0.06, edge];
  const thigh = unit(sub(corner, hip));
  const knee = add(hip, mul(thigh, THIGH));
  const front = unit([0, thigh[2], -thigh[1]]); // перед бедра: вперёд, когда нога висит, к земле, когда лежит
  const shin = add(mul(thigh, Math.cos(DRAPE_BEND)), mul(front, -Math.sin(DRAPE_BEND)));
  return { knee: add(knee, mul(front, 0.1)), foot: add(knee, mul(shin, SHIN)) };
}
function leg(L, s, t, spread) {
  const out = s === 'Left' ? 1 : -1;
  const hip = globals(L)[id(s + 'UpLeg')].p;
  const k = knees[s](t);
  const w = drape[s](t);
  const bones = ['UpLeg', 'Leg', 'Foot'].map((n) => id(s + n));
  reach(L, ...bones, feet[s](t), add(hip, [out * k[0], k[1], k[2]]), out * spread);
  aim(L, bones[2], id(s + 'ToeBase'), unit(toes[s](t)));
  if (w > 0) {
    // Нога на кромке считается отдельно, потом повороты костей смешиваются: без перескока колена в переходе.
    const D = L.map((l) => ({ ...l }));
    const h = hanging(hip);
    reach(D, ...bones, h.foot, h.knee);
    aim(D, bones[2], id(s + 'ToeBase'), unit(toes[s](t)));
    for (const i of bones) L[i] = { ...L[i], r: slerp(L[i].r, D[i].r, w) };
  }
}
// Отвод локтя или колена вокруг линии к опоре по кадрам: путь по всем кадрам, на котором рука или нога меньше
// всего входит в землю и отвод меняется плавно (штраф за поворот между кадрами), малый отвод лучше большого.
const SPREADS = {
  Arm: [...Array(7)].map((_, i) => -30 + i * 10),
  Leg: [...Array(5)].map((_, i) => -30 + i * 10),
};
const STEER = 0.0004; // штраф за каждые 10° смены отвода между соседними кадрами
function sinkOf(t, s, kind, spread) {
  const L = body(t);
  (kind === 'Arm' ? arm : leg)(L, s, t, spread);
  return limbSink(L, s, kind) + 0.00002 * Math.abs(spread);
}
const duration = LENGTH + (north ? NORTH_RIM_DELAY : 0);
const frames = Math.round(duration * FPS);
const times = [...Array(frames + 1)].map((_, f) => f / FPS);
// После хвата за кромку фаза северного клипа идёт с темпом бокового подъёма.
const poseTimes = north
  ? times.map(track([[0, 0], [3.7, 1.75], [4.7, 2.6], [5, 2.9], [duration, LENGTH]]))
  : times;
const spreads = {};
for (const s of SIDES) for (const kind of ['Arm', 'Leg']) {
  const range = SPREADS[kind];
  const cost = poseTimes.map((t) => range.map((spread) => sinkOf(t, s, kind, spread)));
  const total = [cost[0].slice()], from = [range.map(() => 0)];
  for (let f = 1; f < cost.length; f++) {
    total.push([]); from.push([]);
    for (let j = 0; j < range.length; j++) {
      let best = Infinity, arg = 0;
      for (let i = 0; i < range.length; i++) {
        const c = total[f - 1][i] + STEER * Math.abs(i - j);
        if (c < best) { best = c; arg = i; }
      }
      total[f][j] = best + cost[f][j];
      from[f][j] = arg;
    }
  }
  let j = total[cost.length - 1].indexOf(Math.min(...total[cost.length - 1]));
  const path = [];
  for (let f = cost.length - 1; f >= 0; f--) { path.unshift(range[j]); j = from[f][j]; }
  spreads[s + kind] = path.map((_, i) => {
    const part = path.slice(Math.max(0, i - 8), i + 9);
    return part.reduce((x, y) => x + y, 0) / part.length;
  });
}

function pose(t, f) {
  const L = body(t);
  for (const s of SIDES) {
    arm(L, s, t, spreads[s + 'Arm'][f]);
    leg(L, s, t, spreads[s + 'Leg'][f]);
  }
  const w = smooth(BLEND_FROM, LENGTH, t);
  if (w > 0) {
    const endHips = [baseHipsP[0], baseHipsP[1], END_Z];
    return L.map((l, i) => (i === HIPS
      ? { r: slerp(l.r, base[i].r, w), p: l.p.map((x, k) => x + (endHips[k] - x) * w), s: l.s }
      : { r: slerp(l.r, base[i].r, w), p: l.p, s: l.s }));
  }
  return L;
}

const poses = times.map((t, f) => ({ t, poseTime: poseTimes[f], L: pose(poseTimes[f], f) }));

// Замеры: промах кистей и стоп до цели, провал рук и ног в землю, наибольший скачок кости за кадр.
const report = { handMiss: 0, footMiss: 0, limbSink: 0, limbSinkAt: 0, jump: 0, jumpAt: 0, kneeBack: 0, kneeBackAt: 0 };
const restTurn = JSON.parse(fs.readFileSync(FILE).slice(20, 20 + fs.readFileSync(FILE).readUInt32LE(12))).nodes.map((n) => n.rotation ?? [0, 0, 0, 1]);
let prev = null;
for (const { t, poseTime, L } of poses) {
  const Gs = globals(L);
  if (poseTime < BLEND_FROM) for (const s of SIDES) {
    report.handMiss = Math.max(report.handMiss, norm(sub(Gs[id(s + 'Hand')].p, hands[s](poseTime))));
    report.footMiss = Math.max(report.footMiss, norm(sub(Gs[id(s + 'Foot')].p, feet[s](poseTime))));
  }
  for (const s of SIDES) {
    // Колено этой модели сгибается поворотом голени вокруг её оси Z в плюс; минус — колено вывернуто назад.
    const i = id(s + 'Leg');
    const back = -toEuler(qmul(qinv(restTurn[i]), L[i].r))[2];
    if (back > report.kneeBack) { report.kneeBack = back; report.kneeBackAt = t; }
  }
  for (const s of SIDES) for (const kind of ['Arm', 'Leg']) {
    const d = limbSink(L, s, kind);
    if (d > report.limbSink) { report.limbSink = d; report.limbSinkAt = t; }
  }
  if (prev) for (let i = 0; i < Gs.length; i++) {
    const j = norm(sub(Gs[i].p, prev[i].p));
    if (j > report.jump) { report.jump = j; report.jumpAt = t; }
    if (process.env.JUMPS && j > 0.03) console.log('скачок', t.toFixed(3), j.toFixed(3), names[i]);
  }
  prev = Gs;
}

// Запись: ключ на каждый кадр для всех каналов исходного клипа; старый клип с тем же именем заменяется,
// данные, на которые больше никто не ссылается, выбрасываются из буфера.
const raw = fs.readFileSync(FILE);
const jsonLength = raw.readUInt32LE(12);
const doc = JSON.parse(raw.slice(20, 20 + jsonLength));
const bin = raw.slice(28 + jsonLength);
const viewOf = (i) => { const v = doc.bufferViews[i]; return bin.slice(v.byteOffset ?? 0, (v.byteOffset ?? 0) + v.byteLength); };
const source = doc.animations.find((a) => a.name === BASE_CLIP);
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
const at = doc.animations.findIndex((a) => a.name === TARGET);
if (at >= 0) doc.animations[at] = target; else doc.animations.push(target);

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
const bytes = Buffer.concat(chunks);
const header = Buffer.alloc(12);
header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4);
header.writeUInt32LE(12 + 8 + json.length + 8 + bytes.length, 8);
const chunkHeader = (len, type) => { const b = Buffer.alloc(8); b.writeUInt32LE(len, 0); b.writeUInt32LE(type, 4); return b; };
if (!process.env.DRY) fs.writeFileSync(FILE, Buffer.concat([header, chunkHeader(json.length, 0x4e4f534a), json, chunkHeader(bytes.length, 0x004e4942), bytes]));

console.log(JSON.stringify({ clip: TARGET, seconds: duration, keys: poses.length, ...Object.fromEntries(Object.entries(report).map(([k, v]) => [k, +v.toFixed(3)])) }));
if (process.env.ROWS) for (const { t, poseTime, L } of poses.filter((_, i) => i % 6 === 0)) {
  const Gs = globals(L);
  const P = (n) => Gs[id(n)].p;
  const f = (v) => v.map((x) => x.toFixed(2)).join(',');
  const miss = (s, part, tr) => norm(sub(P(s + part), tr[s](poseTime))).toFixed(2);
  console.log(t.toFixed(1), 'плечоЛ', f(P('LeftArm')), 'промах кисть', miss('Left', 'Hand', hands), miss('Right', 'Hand', hands),
    'стопа', miss('Left', 'Foot', feet), miss('Right', 'Foot', feet), 'бедро', f(P('LeftUpLeg')), 'коленоЛ', f(P('LeftLeg')), 'коленоП', f(P('RightLeg')), 'стопы', f(P('LeftFoot')), f(P('RightFoot')), 'голова', f(P('HeadTop_End')));
}
if (process.env.ELBOW) for (const [f, { t, L }] of poses.entries()) {
  if (t < +process.env.ELBOW - 0.1 || t > +process.env.ELBOW + 0.1) continue;
  const Gs = globals(L);
  const P = (n) => Gs[id(n)].p.map((x) => x.toFixed(3)).join(',');
  console.log(t.toFixed(3), 'отвод', spreads.LeftArm[f].toFixed(1), spreads.RightArm[f].toFixed(1), 'плечо', P('LeftArm'), 'локоть', P('LeftForeArm'), 'кисть', P('LeftHand'));
}
if (process.env.KNEES) for (let f = 0; f < poses.length; f += 6) {
  const { t, L } = poses[f];
  console.log(t.toFixed(1), SIDES.map((s) => { const i = id(s + 'Leg'); return toEuler(qmul(qinv(restTurn[i]), L[i].r))[2].toFixed(0); }).join(' '));
}
