// Карты кусков грунтовых дорог Транспортной компании («Фаза-43-вид-дороги»): цвет с прозрачностью, мелкий
// рельеф и шероховатость прямого куска каждого вида и перекрёстков («Фаза-46-перекрёстки-готовыми-моделями»).
// Основа — грунт ambientCG Ground103 в настоящем размере; поверх — колеи там, где ходят колёса, островки травы, куда колесо не наступает, и неровный край,
// уходящий в землю. Колеи — широкие мягкие ложбины разной силы, каждая виляет сама: ровные тёмные полосы
// читаются сверху как рельсы.
// Картинка куска: по ширине — поперёк дороги, по высоте — вдоль, от начала куска к концу. Всё общее у вариантов
// повторяется с длиной куска, а своё у варианта гаснет к концам: варианты стыкуются в любом порядке без шва.
// Запуск из tools/art: node roads.mjs [папка]  — по умолчанию art/transport/textures.

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const HERE = dirname(fileURLToPath(import.meta.url));
const GROUND = resolve(HERE, "../../art/transport/textures/Ground103");
const TILE = 2.5;            // Ground103 покрывает 2,5 × 2,5 метра
const ACROSS = 512;          // точек куска поперёк
const ALONG = 1024;          // точек куска вдоль

// Виды дорог в метрах: ширина и длина куска; колёса — где ходят, насколько глубока колея (0–1) и её ширина;
// полуширина укатанного грунта и ширина края; трава — сколько её на островках между колёсами.
const KINDS = {
  // Главная улица и тракт: широкая, вытоптанная, колеи двух сторон расплылись в полосы.
  main: { width: 12, length: 16, wheels: [-3.8, -2.2, 2.2, 3.8], rut: 0.45, rutWidth: 0.9, worn: 4.5, edge: 1.5, grass: 0.15 },
  // Улица: колеи двух сторон, между сторонами — островки травы.
  street: { width: 7, length: 14, wheels: [-2.5, -1.0, 1.0, 2.5], rut: 0.55, rutWidth: 0.6, worn: 2.6, edge: 0.9, grass: 0.55 },
  // Проезд: одна пара колей, трава между ними.
  drive: { width: 4.5, length: 9, wheels: [-0.75, 0.75], rut: 0.85, rutWidth: 0.45, worn: 1.3, edge: 0.7, grass: 0.9 },
};
const VARIANTS = ["a", "b"];

// Шум по решётке: значения в узлах из хэша, между ними — сглаженно; по `y` повторяется с периодом `py` узлов.
function hash(x, y, seed) {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
const smooth = (t) => t * t * (3 - 2 * t);
function lattice(x, y, py, seed) {
  const ix = Math.floor(x), iy = Math.floor(y);
  const fx = smooth(x - ix), fy = smooth(y - iy);
  const at = (i, j) => hash(i, ((j % py) + py) % py, seed);
  const top = at(ix, iy) * (1 - fx) + at(ix + 1, iy) * fx;
  const bottom = at(ix, iy + 1) * (1 - fx) + at(ix + 1, iy + 1) * fx;
  return top * (1 - fy) + bottom * fy;
}
// Несколько октав от −1 до 1; `x`, `y` — в метрах, `cell` — шаг крупной октавы, по `y` период `period` метров.
function fbm(x, y, cell, period, seed, octaves = 3) {
  let sum = 0, amp = 1, norm = 0, c = cell;
  for (let o = 0; o < octaves; o++) {
    const py = Math.max(1, Math.round(period / c));
    sum += amp * (lattice(x / c, (y / period) * py, py, seed + o * 101) * 2 - 1);
    norm += amp;
    amp *= 0.5;
    c /= 2;
  }
  return sum / norm;
}

async function readMap(name) {
  const { data, info } = await sharp(join(GROUND, name)).raw().toBuffer({ resolveWithObject: true });
  return { data, w: info.width, h: info.height, ch: info.channels };
}
// Точка карты основы по метрам, с повтором и сглаживанием; `tile` — сколько метров покрывает карта.
function sampler(map, tile) {
  return (xm, ym, c) => {
    const fx = ((((xm / tile) % 1) + 1) % 1) * map.w - 0.5, fy = ((((ym / tile) % 1) + 1) % 1) * map.h - 0.5;
    const x0 = Math.floor(fx), y0 = Math.floor(fy), tx = fx - x0, ty = fy - y0;
    const px = (x, y) => map.data[((((y % map.h) + map.h) % map.h) * map.w + (((x % map.w) + map.w) % map.w)) * map.ch + c];
    return (px(x0, y0) * (1 - tx) + px(x0 + 1, y0) * tx) * (1 - ty) + (px(x0, y0 + 1) * (1 - tx) + px(x0 + 1, y0 + 1) * tx) * ty;
  };
}

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const step = (a, b, v) => smooth(clamp((v - a) / (b - a)));

/** Карты одной точки грунта: цвет основы, тон, затемнение колеёй, прозрачность, шероховатость, высота. */
function texel(maps, p, sample, xm, ym, look) {
  const { rgba, nrm, orm, height } = maps;
  for (let c = 0; c < 3; c++) rgba[p * 4 + c] = clamp((sample.color(xm, ym, c) / 255) * look.shade, 0, 1) * 255;
  rgba[p * 4 + 3] = clamp(look.alpha) * 255;
  orm[p * 3] = 255;
  orm[p * 3 + 1] = clamp(sample.arm(xm, ym, 1) / 255 - look.smooth, 0.62, 1) * 255;
  orm[p * 3 + 2] = 0;
  for (let c = 0; c < 3; c++) nrm[p * 3 + c] = sample.normal(xm, ym, c);
  height[p] = look.height;
}

function newMaps(w, h) {
  return { rgba: Buffer.alloc(w * h * 4), nrm: Buffer.alloc(w * h * 3), orm: Buffer.alloc(w * h * 3), height: new Float32Array(w * h), w, h };
}

function samplerSet(ground, tile) {
  return { color: sampler(ground.color, tile), normal: sampler(ground.normal, tile), arm: sampler(ground.arm, tile) };
}

/** Тон грунта, общий у кусков и узла: крупные пятна светлее и темнее. */
const tone = (n) => 1.16 + 0.07 * n;

/** Карты куска вида `kind`, вариант `variant`. */
function piece(kind, variant, ground) {
  const k = KINDS[kind];
  const sample = samplerSet(ground, k.length / Math.max(1, Math.round(k.length / TILE)));  // целое число повторов основы на кусок
  const own = 17 + VARIANTS.indexOf(variant) * 1000;
  const maps = newMaps(ACROSS, ALONG);
  const dx = k.width / ACROSS, dy = k.length / ALONG;
  for (let j = 0; j < ALONG; j++) {
    const y = (j + 0.5) * dy;
    const window = Math.sin((Math.PI * y) / k.length) ** 2;   // своё у варианта гаснет к концам
    // Шум варианта к концам куска сменяется общим.
    const varied = (x, cell, seed, oct = 3) => fbm(x, y, cell, k.length, seed, oct) * (1 - window) + fbm(x, y, cell, k.length, own + seed, oct) * window;
    // У каждого колеса свой ход, сила колеи и ширина меняются вдоль.
    const wheels = k.wheels.map((w, n) => ({
      at: w + 0.18 * varied(n * 7, 4.5, 60 + n, 2),
      depth: k.rut * clamp(0.65 + 0.55 * varied(n * 7, 3, 70 + n, 2)),
      half: (k.rutWidth / 2) * (0.8 + 0.35 * varied(n * 7, 2.5, 80 + n, 2)),
    }));
    const edgeShift = 0.35 * varied(5, 3, 7, 3);
    for (let i = 0; i < ACROSS; i++) {
      const x = (i + 0.5) * dx - k.width / 2;
      const p = j * ACROSS + i;
      let rut = 0, near = Infinity;
      for (const w of wheels) {
        const d = Math.abs(x - w.at);
        rut = Math.max(rut, w.depth * Math.exp(-((d / w.half) ** 2)));
        near = Math.min(near, d - w.half);
      }
      // Трава островками там, куда колесо не наступает: дальше полутора колей от колеса и внутри укатанного.
      const away = step(0.1, 0.6, near);
      const patch = clamp((varied(x, 1.4, 11) + 0.15) * 2.4) * clamp(0.6 + 0.8 * varied(x, 0.3, 13, 2));
      const grass = k.grass * away * patch;
      // Край: укатанный грунт до `worn`, дальше клочьями уходит в землю.
      const reach = k.worn + edgeShift + 0.3 * varied(x, 0.7, 5);
      const edge = 1 - step(reach, reach + k.edge * 0.7, Math.abs(x));
      const border = 1 - step(k.width / 2 - 0.2, k.width / 2, Math.abs(x));
      const damp = rut * window * clamp(varied(x, 2, 23) * 2.5 - 0.4);
      texel(maps, p, sample, x + k.width / 2, y, {
        shade: tone(varied(x, 5, 31)) * (1 - 0.24 * rut) * (1 - 0.18 * damp),
        alpha: edge * border * (1 - grass),
        smooth: 0.05 * rut + 0.2 * damp,
        height: -0.05 * rut,
      });
    }
  }
  addSlopes(maps, dx, dy, true);
  return maps;
}

// Перекрёстки готовыми моделями: тупик, переход одной улицы в другую (и в улицу того же вида другой ширины —
// рукава движок сужает под их дороги), поворот, буква Т и крест для всех сочетаний видов. Рукава под прямым углом — движок потом изгибает их под настоящий угол улиц. Углы между рукавами скруглены;
// рукав кончается своим устьем, а последнюю долю ширины грунт гаснет — туда заходят куски улицы, и шва нет. Пути
// колёс — при правостороннем движении: сквозные, повороты направо (тесные у угла) и налево (широкие через середину),
// разворот в тупике. Рядом с картинками каждого перекрёстка ложится shape.json — рукава, контур сетки и рамка
// картинки: по нему art/transport/models/roads.py собирает модель.
const JUNCTION_CORNER = 5 / 12;  // радиус скругления угла — доля средней ширины двух рукавов
const JUNCTION_FADE = 4 / 12;    // последнюю долю своей ширины рукав гаснет поверх кусков улицы
const JUNCTION_LINE = 0.75;      // у перехода одной улицы в другую рукава начинаются в этой доле ширины широкой
const JUNCTION_MARGIN = 0.5;     // метров: сетка модели шире кромки, чтобы не обрезать гаснущий край
const JUNCTION_PX = 0.04;        // метров на точку картинки
const JUNCTION_MAX = 1024;       // точек по стороне, не больше
const JUNCTION_ARC = 12;         // отрезков на четверть окружности контура
const JUNCTION_CURVE = 32;       // отрезков на кривую пути колёс
// Сила колеи по роду пути; сквозные к середине размешаны встречным движением.
const JUNCTION_PATH = { through: 1, bend: 1, right: 1.15, left: 0.9, back: 1.1 };
const THROUGH_IN_CORE = 0.3;
const KIND_ORDER = ["main", "street", "drive"];

/** Перекрёстки набора: имя → рукава [направление в градусах против часовой от востока (+X Blender), вид]. */
function junctionSet() {
  const set = [];
  KIND_ORDER.forEach((a, ia) => {
    set.push({ name: `road_end_${a}`, arms: [[0, a]] });
    KIND_ORDER.forEach((b, ib) => {
      set.push({ name: `road_tee_${a}_${b}`, arms: [[0, a], [180, a], [90, b]] });
      if (ib < ia) return;
      set.push({ name: `road_bend_${a}_${b}`, arms: [[0, a], [90, b]] });
      set.push({ name: `road_cross_${a}_${b}`, arms: [[0, a], [180, a], [90, b], [270, b]] });
      set.push({ name: `road_line_${a}_${b}`, arms: [[0, a], [180, b]] });
    });
  });
  return set;
}

const rot = (v, a) => [v[0] * Math.cos(a) - v[1] * Math.sin(a), v[0] * Math.sin(a) + v[1] * Math.cos(a)];
const add = (a, b, s = 1) => [a[0] + b[0] * s, a[1] + b[1] * s];

/** Рукав: направление, вид и его полоса — середина колеи `lane` от оси и ширина колеи `track`. */
function arm(dir, kind) {
  const k = KINDS[kind];
  const rad = (dir * Math.PI) / 180;
  const two = k.wheels.length === 2;
  return {
    dir, rad, kind, k, w: k.width, h: k.width / 2,
    e: [Math.cos(rad), Math.sin(rad)], n: [-Math.sin(rad), Math.cos(rad)],
    lane: two ? 0 : (k.wheels[2] + k.wheels[3]) / 2,
    track: two ? k.wheels[1] - k.wheels[0] : k.wheels[3] - k.wheels[2],
  };
}

/** Угол от рукава `a` до `b` против часовой, градусов, в (0, 360]. */
const gap = (a, b) => ((b.dir - a.dir + 360) % 360) || 360;

/** Где начинаются рукава и где кончаются: `start` — за скруглениями углов, `reach` — устье, `fade` — длина гаснущей
 *  полосы перед устьем. */
function plan(arms) {
  const W = Math.max(...arms.map((a) => a.w));
  const n = arms.length;
  const radius = (a, b) => JUNCTION_CORNER * (a.w + b.w) / 2;
  arms.forEach((a, i) => {
    // Скругление слева — с соседом против часовой, справа — с соседом по часовой, если между ними 90°.
    const prev = arms[(i + n - 1) % n], next = arms[(i + 1) % n];
    const corners = [];
    if (n > 1 && gap(a, next) === 90) corners.push(next.h + radius(a, next));
    if (n > 1 && gap(prev, a) === 90) corners.push(prev.h + radius(prev, a));
    a.start = corners.length ? Math.max(...corners) : n === 1 ? a.h : JUNCTION_LINE * W;
    a.fade = JUNCTION_FADE * a.w;
    a.reach = a.start + a.fade;
  });
  return { W, radius };
}

/** Контур перекрёстка против часовой: для каждого рукава — углы устья, затем сторона до следующего рукава. Устья
 *  помечены: краем они не считаются. `m` — на сколько метров контур шире кромки. */
function ring(arms, radius, m) {
  const pts = [];
  const at = (a, s, side) => add(add([0, 0], a.e, s), a.n, side * (a.h + m));
  const n = arms.length;
  arms.forEach((a, i) => {
    const b = arms[(i + 1) % n];
    pts.push({ p: at(a, a.reach, -1), mouth: true }, { p: at(a, a.reach, 1), mouth: false });
    const g = gap(a, b);
    if (g === 90) {
      const r = radius(a, b);
      const c = add(add([0, 0], a.e, b.h + r), a.n, a.h + r);
      const rr = r - m;
      for (let q = 0; q <= JUNCTION_ARC; q++) {
        const t = a.rad - Math.PI / 2 - (Math.PI / 2) * (q / JUNCTION_ARC);
        pts.push({ p: [c[0] + rr * Math.cos(t), c[1] + rr * Math.sin(t)], mouth: false });
      }
    } else if (g === 180) {
      pts.push({ p: at(a, a.start, 1), mouth: false }, { p: at(b, b.start, -1), mouth: false });
    } else {
      // Внешняя сторона поворота (270°) или тупика (360°): дуга вокруг середины от левой кромки к правой следующего.
      const steps = Math.max(2, Math.round((JUNCTION_ARC * (g - 180)) / 90));
      for (let q = 0; q <= steps; q++) {
        const f = q / steps;
        const t = a.rad + Math.PI / 2 + ((g - 180) * Math.PI / 180) * f;
        const rr = a.h + (b.h - a.h) * f + m;
        pts.push({ p: [rr * Math.cos(t), rr * Math.sin(t)], mouth: false });
      }
    }
  });
  return pts;
}

/** Ломаная пути колеса: прямо по рукаву `a` от устья до начала, кривой к началу рукава `b`, прямо до его устья.
 *  `side` — колесо правое (+1) или левое (−1) по ходу. */
function wheelPath(a, b, side) {
  const p0 = add(add([0, 0], a.e, a.start), a.n, a.lane + side * a.track / 2);
  const p3 = add(add([0, 0], b.e, b.start), b.n, -(b.lane + side * b.track / 2));
  const span = Math.hypot(p3[0] - p0[0], p3[1] - p0[1]);
  const c = a === b ? 1.3 * Math.max(a.lane, 1) + a.h * 0.6 : 0.55 * span;
  const p1 = add(p0, a.e, -c), p2 = add(p3, b.e, -c);
  const pts = [add(p0, a.e, a.reach - a.start)];
  for (let q = 0; q <= JUNCTION_CURVE; q++) {
    const t = q / JUNCTION_CURVE, u = 1 - t;
    pts.push([0, 1].map((k) => u * u * u * p0[k] + 3 * u * u * t * p1[k] + 3 * u * t * t * p2[k] + t * t * t * p3[k]));
  }
  pts.push(add(p3, b.e, b.reach - b.start));
  return pts;
}

/** Пути колёс перекрёстка с их родом: из каждого рукава в каждый, в тупике — разворот. */
function junctionPaths(arms) {
  const paths = [];
  for (const a of arms) {
    for (const b of arms) {
      let kind;
      if (a === b) {
        if (arms.length > 1) continue;
        kind = "back";
      } else if (gap(a, b) === 180) kind = "through";
      else if (arms.length === 2) kind = "bend";
      // Въехав из рукава `a` (по −e) и выехав в `b` (по +e), направо — значит повернуть по часовой.
      else kind = a.e[0] * b.e[1] - a.e[1] * b.e[0] > 0 ? "right" : "left";
      for (const side of [1, -1]) {
        const pts = wheelPath(a, b, side);
        const lens = [0];
        for (let q = 1; q < pts.length; q++) lens.push(lens[q - 1] + Math.hypot(pts[q][0] - pts[q - 1][0], pts[q][1] - pts[q - 1][1]));
        const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
        paths.push({ kind, pts, lens, box: [Math.min(...xs) - 2, Math.min(...ys) - 2, Math.max(...xs) + 2, Math.max(...ys) + 2] });
      }
    }
  }
  return paths;
}

/** Смещение точки от ломаной вбок со знаком и пройденная до ближайшей её точки длина. */
function offsetAlong(path, x, y) {
  let best = Infinity, sign = 1, along = 0;
  const { pts, lens } = path;
  for (let q = 1; q < pts.length; q++) {
    const [ax, ay] = pts[q - 1], [bx, by] = pts[q];
    const dx = bx - ax, dy = by - ay, len2 = dx * dx + dy * dy || 1e-9;
    const t = clamp(((x - ax) * dx + (y - ay) * dy) / len2);
    const px = ax + dx * t, py = ay + dy * t;
    const d = Math.hypot(x - px, y - py);
    if (d < best) {
      best = d;
      sign = dx * (y - ay) - dy * (x - ax) >= 0 ? 1 : -1;
      along = lens[q - 1] + Math.sqrt(len2) * t;
    }
  }
  return [sign * best, along];
}

/** Расстояние от точки до отрезка. */
function segDistance(x, y, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1], len2 = dx * dx + dy * dy || 1e-9;
  const t = clamp(((x - a[0]) * dx + (y - a[1]) * dy) / len2);
  return Math.hypot(x - a[0] - dx * t, y - a[1] - dy * t);
}

/** Точка внутри многоугольника (чётность пересечений). */
function insidePolygon(x, y, poly) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Карты перекрёстка и его shape.json. */
function junction(spec, ground) {
  const arms = spec.arms.map(([dir, kind]) => arm(dir, kind)).sort((a, b) => a.dir - b.dir);
  const { W, radius } = plan(arms);
  const edgeRing = ring(arms, radius, 0);
  const poly = edgeRing.map((v) => v.p);
  const edges = [];
  edgeRing.forEach((v, i) => { if (!v.mouth) edges.push([v.p, edgeRing[(i + 1) % edgeRing.length].p]); });
  const outline = ring(arms, radius, JUNCTION_MARGIN).map((v) => v.p);
  const xs = outline.map((p) => p[0]), ys = outline.map((p) => p[1]);
  const bounds = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
  const size = [bounds[2] - bounds[0], bounds[3] - bounds[1]];
  const px = Math.max(JUNCTION_PX, Math.max(...size) / JUNCTION_MAX);
  const [iw, ih] = size.map((s) => Math.ceil(s / px));
  const dx = size[0] / iw, dy = size[1] / ih;
  const paths = junctionPaths(arms);
  const hasCore = arms.length !== 2 || gap(arms[0], arms[1]) !== 180;
  const widest = arms.reduce((a, b) => (b.w > a.w ? b : a));
  const rCore = arms.length > 1 ? JUNCTION_CORNER * W : 0;
  const noise = (x, y, cell, seed, oct = 3) => fbm(x, y, cell, 1000, seed, oct);
  const sample = samplerSet(ground, TILE);
  const maps = newMaps(iw, ih);
  for (let j = 0; j < ih; j++) {
    for (let i = 0; i < iw; i++) {
      const x = bounds[0] + (i + 0.5) * dx, y = bounds[3] - (j + 0.5) * dy;
      const inside = insidePolygon(x, y, poly);
      let toEdge = Infinity;
      for (const [a, b] of edges) toEdge = Math.min(toEdge, segDistance(x, y, a, b));
      const dB = inside ? toEdge : -toEdge;
      // Вид ближнего по углу рукава даёт ширину края, траву и колею.
      const rr = Math.hypot(x, y);
      const own = arms.reduce((best, a) => ((x * a.e[0] + y * a.e[1]) / (rr || 1) > (x * best.e[0] + y * best.e[1]) / (rr || 1) ? a : best));
      const k = rr < 0.5 * widest.h ? widest.k : own.k;
      let rut = 0, near = Infinity;
      paths.forEach((p, n) => {
        if (x < p.box[0] || y < p.box[1] || x > p.box[2] || y > p.box[3]) return;
        const [offset, along] = offsetAlong(p, x, y);
        if (Math.abs(offset) > 2) return;
        const pk = KINDS[own.kind];
        const wiggle = 0.18 * noise(along, n * 7, 4.5, 60 + n, 2);
        const half = (pk.rutWidth / 2) * (0.8 + 0.35 * noise(along, n * 7, 2.5, 80 + n, 2));
        const core = p.kind === "through" && hasCore ? THROUGH_IN_CORE + (1 - THROUGH_IN_CORE) * step(widest.h * 0.5, widest.h + rCore, rr) : 1;
        const depth = pk.rut * JUNCTION_PATH[p.kind] * core * clamp(0.65 + 0.55 * noise(along, n * 7, 3, 70 + n, 2));
        const dd = Math.abs(offset - wiggle);
        rut = Math.max(rut, depth * Math.exp(-((dd / half) ** 2)));
        near = Math.min(near, dd - half);
      });
      // Вне полос рукавов — в скруглённых углах — колёса не ездят: там чаще пробивается трава.
      let out = Infinity;
      for (const a of arms) {
        const s = x * a.e[0] + y * a.e[1], l = x * a.n[0] + y * a.n[1];
        if (s > 0) out = Math.min(out, Math.abs(l) - a.h);
      }
      const corner = rCore > 0 && out < Infinity ? step(0, rCore * 0.5, out) : 0;
      const away = step(0.1, 0.6, near);
      const patch = clamp((noise(x, y, 1.4, 11) + 0.15) * 2.4) * clamp(0.6 + 0.8 * noise(x, y, 0.3, 13, 2));
      const grass = Math.max(k.grass, 0.7 * corner) * away * patch;
      const reach = k.worn + 0.35 * noise(x, y, 3, 7, 3) + 0.3 * noise(x, y, 0.7, 5);
      const edge = 1 - step(reach, reach + k.edge * 0.7, k.width / 2 - dB);
      let fade = 1;
      for (const a of arms) {
        const s = x * a.e[0] + y * a.e[1], l = x * a.n[0] + y * a.n[1];
        if (Math.abs(l) <= a.h + 1) fade *= 1 - step(a.reach - a.fade, a.reach - 0.15, s);
      }
      // Середина утоптана колёсами со всех сторон: светлее и глаже; сырость — только пятнами по колеям.
      const middle = hasCore ? 1 - step(widest.h * 0.4, widest.h + rCore * 0.5, rr) : 0;
      const trodden = middle * clamp(0.6 + 0.8 * noise(x, y, 3, 37));
      const damp = rut * clamp(noise(x, y, 2, 29) * 2.5 - 0.4);
      texel(maps, j * iw + i, sample, x - bounds[0], y - bounds[1], {
        shade: tone(noise(x, y, 5, 31)) * (1 - 0.24 * rut) * (1 - 0.18 * damp) * (1 + 0.06 * trodden),
        alpha: edge * fade * (1 - grass),
        smooth: 0.05 * rut + 0.2 * damp + 0.12 * trodden,
        height: -0.05 * rut,
      });
    }
  }
  addSlopes(maps, dx, dy, false);
  const shape = {
    name: spec.name,
    arms: arms.map((a) => ({ dir: a.dir, kind: a.kind, width: a.w, start: a.start, reach: a.reach, fade: a.fade })),
    outline,
    bounds,
  };
  return { maps, shape };
}

/** Наклоны высоты добавляются к карте рельефа: +X — вправо по картинке, +Y — вверх по картинке, как у glTF;
 *  `wrap` — вдоль картинка повторяется. */
function addSlopes(maps, dx, dy, wrap) {
  const { nrm, height, w, h } = maps;
  const row = (j) => (wrap ? (j + h) % h : Math.min(h - 1, Math.max(0, j)));
  for (let j = 0; j < h; j++) {
    for (let i = 0; i < w; i++) {
      const p = j * w + i;
      const hx = (height[j * w + Math.min(w - 1, i + 1)] - height[j * w + Math.max(0, i - 1)]) / (2 * dx);
      const hy = (height[row(j - 1) * w + i] - height[row(j + 1) * w + i]) / (2 * dy);
      let nx = nrm[p * 3] / 127.5 - 1 - hx, ny = nrm[p * 3 + 1] / 127.5 - 1 - hy, nz = nrm[p * 3 + 2] / 127.5 - 1;
      const len = Math.hypot(nx, ny, nz);
      nrm[p * 3] = (nx / len + 1) * 127.5;
      nrm[p * 3 + 1] = (ny / len + 1) * 127.5;
      nrm[p * 3 + 2] = (nz / len + 1) * 127.5;
    }
  }
}

async function write(dir, maps) {
  await mkdir(dir, { recursive: true });
  const raw = (channels) => ({ raw: { width: maps.w, height: maps.h, channels } });
  await sharp(maps.rgba, raw(4)).png({ compressionLevel: 9 }).toFile(join(dir, "color.png"));
  await sharp(maps.nrm, raw(3)).jpeg({ quality: 92 }).toFile(join(dir, "normal.jpg"));
  await sharp(maps.orm, raw(3)).jpeg({ quality: 92 }).toFile(join(dir, "arm.jpg"));
}

const out = resolve(process.argv[2] ?? resolve(HERE, "../../art/transport/textures"));
const ground = { color: await readMap("color.jpg"), normal: await readMap("normal.jpg"), arm: await readMap("arm.jpg") };
for (const kind of Object.keys(KINDS)) {
  for (const variant of VARIANTS) await write(join(out, `road_${kind}_${variant}`), piece(kind, variant, ground));
}
// Список перекрёстков для `scene.road_junctions` в game.json: рукава моделей с устьем и гаснущей полосой, метров.
const table = {};
const round = (v) => Math.round(v * 1000) / 1000;
for (const spec of junctionSet()) {
  const { maps, shape } = junction(spec, ground);
  await write(join(out, spec.name), maps);
  await writeFile(join(out, spec.name, "shape.json"), JSON.stringify(shape));
  table[spec.name] = { arms: shape.arms.map((a) => ({ dir: a.dir, kind: a.kind, reach: round(a.reach), fade: round(a.fade) })) };
}
await writeFile(join(out, "road_junctions.json"), JSON.stringify(table, null, 2));
console.log(`карты дорог — в ${out}`);
