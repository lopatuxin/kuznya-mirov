// Зародыш города Транспортной компании («Фаза-04-зародыш-города»): программа чертит уездный городок по
// регулярному плану и отдаёт его переводу карты тем же языком, что выгрузка OpenStreetMap. Волга идёт вдоль
// северного края, с юга в неё впадает приток. У берега — площадь с собором, торговыми рядами и рынком, вокруг —
// прямые улицы кварталами, по кварталу — усадьбы: дом лицом к улице, во дворе сарай. От площади расходятся
// тракты, на юге — тупиковая ветка железной дороги со станцией. Кварталы застраиваются по очереди от площади;
// зародыш — первые `town.stage` кварталов, следующий размечен под стройку. Всё считается из `town.seed`: одни и
// те же настройки дают ту же карту.
// Настройки зародыша — поправки поверх настроек Кинешмы (`base`): тот же вид улиц, домов, деревьев и моделей.
// Запуск: node tools/city/seed.mjs art/transport/seed.json

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { buildCity, modelFrame } from "./city.mjs";
import { nearestOnSegment, projector, random, round } from "./geo.mjs";
import { writeCity } from "./osm2scene.mjs";

// Размеры в метрах.
const BLOCK_U = 120;         // квартал вдоль реки
const BLOCK_V = 100;         // квартал от реки
const PLOT = 22;             // усадьба вдоль улицы
const SLOBODA_PLOT = 24;     // усадьба вдоль тракта
const SETBACK = 3;           // от края улицы до дома
const CORNER = 26;           // угол квартала, занятый усадьбами длинных сторон
const IZBA = [6.5, 8.5];     // изба: по улице × вглубь — щипцом к улице
const SQUARE_HOUSE = [8, 8]; // двухэтажный деревянный или одноэтажный кирпичный дом
const STONE = [14, 10];      // двухэтажный каменный дом у площади
const SHED = [4, 6];
const STONE_REACH = 210;     // каменные дома — не дальше от середины площади
const RIVER_KEEP = 150;      // тракт вдоль Волги — не ближе к берегу
const BRIDGE_MARGIN = 12;    // мост длиннее воды с каждой стороны
const STATION_STREET = 75;   // Станционная улица — севернее путей
const TRACT_PULL = 0.5;      // на сколько кварталов ближе к площади считается квартал вдоль тракта
const TRACT_TAIL = 15;       // тракт кончается за последним, что на нём стоит

// Ряды плана: `j` — улицы вдоль реки от берега, `i` — поперечные от площади (0 — Вокзальная, к станции).
const ALONG = ["Набережная улица", "Соборная улица", "Советская улица", "Садовая улица", "Полевая улица", "Луговая улица", "Огородная улица", "Новая улица", "Лесная улица"];
const ACROSS_WEST = ["Никольская улица", "Пролетарская улица", "Ильинская улица", "Кузнечная улица", "Песочная улица", "Красноармейская улица", "Овражная улица", "Сенная улица", "Конная улица", "Ямская улица"];
const ACROSS_EAST = ["Торговая улица", "Первомайская улица", "Кооперативная улица", "Почтовая улица", "Школьная улица", "Заводская улица", "Колхозная улица", "Церковная улица", "Рыбацкая улица", "Береговая улица"];
const PLAN_A = [-9, 9];      // кварталы плана вдоль реки
const PLAN_B = [0, 7];       // кварталы плана от реки

const add = (p, d, k = 1) => [p[0] + d[0] * k, p[1] + d[1] * k];
const dist = (p, q) => Math.hypot(p[0] - q[0], p[1] - q[1]);
const unit = (d) => { const l = Math.hypot(d[0], d[1]); return [d[0] / l, d[1] / l]; };
const rightOf = (t) => [-t[1], t[0]];  // справа по ходу, если смотреть сверху (y вниз)

/** План зародыша: элементы в виде выгрузки OpenStreetMap, места игры для перевода карты и название городка. */
export function planTown(config) {
  const cell = config.cell;
  const m = (meters) => meters / cell;
  const proj = projector(config.bbox, cell);
  const W = proj.width, H = proj.height;
  const town = config.town;
  const rand = random(town.seed);
  const chosen = town.names[Math.floor(rand() * town.names.length)];
  if (chosen.tracts.length < 2) throw new Error(`у городка «${chosen.name}» меньше двух трактов`);
  const name = chosen.name;
  const tracts = shuffled(chosen.tracts, rand);

  // Берега: Волга по северу, приток с юга с заливом в устье.
  const wave = (amp, len) => {
    const phase = rand() * Math.PI * 2;
    return (s) => amp * Math.sin((2 * Math.PI * s) / len + phase);
  };
  const s1 = wave(m(110), m(5200)), s2 = wave(m(40), m(1700));
  const shore = (x) => m(560) + s1(x) + s2(x);
  const cx = W * (0.38 + rand() * 0.1);
  const t1 = wave(m(70), m(1500)), t2 = wave(m(25), m(520));
  const tribX0 = cx + m(640 + rand() * 120);
  const trib = (y) => tribX0 + t1(y) + t2(y);
  const mouth = shore(tribX0);
  const tribHalf = (y) => m(24) * (1 + 1.4 * Math.exp(-Math.max(0, y - mouth) / m(150)));
  const water = (x, y, margin = 0) => y < shore(x) + margin || Math.abs(x - trib(y)) < tribHalf(y) + margin;
  const inMap = ([x, y], margin) => x > margin && y > margin && x < W - margin && y < H - margin;

  // Сетка плана: оси вдоль берега у площади.
  const slope = (shore(cx + 6) - shore(cx - 6)) / 12;
  const theta = Math.max(-0.14, Math.min(0.14, Math.atan(slope)));
  const U = [Math.cos(theta), Math.sin(theta)];
  const V = [-Math.sin(theta), Math.cos(theta)];
  const O = [cx, shore(cx) + m(70)];
  const at = (u, v) => [O[0] + u * U[0] + v * V[0], O[1] + u * U[1] + v * V[1]];
  const BU = m(BLOCK_U), BV = m(BLOCK_V);

  const blockOk = (a, b) => {
    for (let i = 0; i <= 3; i++) {
      for (let k = 0; k <= 3; k++) {
        const p = at((a + i / 3) * BU, (b + k / 3) * BV);
        if (!inMap(p, 3) || water(p[0], p[1], m(30))) return false;
      }
    }
    return true;
  };
  if (!blockOk(0, 0)) throw new Error("площадь зародыша в воде — смени town.seed");

  // Очередь застройки: каждый раз — соседний с застроенным квартал, ближайший к площади (поперёк реки дальше);
  // кварталы вдоль Соборной и Вокзальной — будущих трактов — раньше прочих.
  const key = (a, b) => a + "," + b;
  const scores = new Map();
  const scoreOf = (a, b) => {
    const k = key(a, b);
    const alongTract = b <= 1 || a === 0 || a === -1;
    if (!scores.has(k)) scores.set(k, Math.hypot(a + 0.5, (b + 0.5) * 1.3) - (alongTract ? TRACT_PULL : 0) + rand() * 0.9);
    return scores.get(k);
  };
  const built = new Set([key(0, 0)]);
  const order = [];
  while (order.length < town.stage + 1) {
    let best = null;
    for (const k of built) {
      const [a, b] = k.split(",").map(Number);
      for (const [da, db] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const na = a + da, nb = b + db;
        if (na < PLAN_A[0] || na > PLAN_A[1] || nb < PLAN_B[0] || nb > PLAN_B[1] || built.has(key(na, nb)) || !blockOk(na, nb)) continue;
        const s = scoreOf(na, nb);
        if (!best || s < best.s) best = { a: na, b: nb, s };
      }
    }
    if (!best) throw new Error("зародышу некуда расти — смени town.seed");
    built.add(key(best.a, best.b));
    order.push(best);
  }
  const blocks = order.slice(0, town.stage);
  const next = order[town.stage];

  // Улицы: стороны застроенных кварталов, площади и размеченного квартала; Соборная и Вокзальная — сплошные.
  const hw = (cls) => m(config.roads[cls].width) / 2;
  const edges = new Map();
  const edgeClass = (dir, line, pos) => {
    if ((dir === "h" && line === 1) || (dir === "v" && line === 0)) return "secondary";
    return (line === 0 || line === 1) && pos === 0 ? "tertiary" : "residential";  // стороны площади шире
  };
  const addEdge = (dir, line, pos) => edges.set(`${dir}:${line}:${pos}`, { dir, line, pos, cls: edgeClass(dir, line, pos) });
  for (const { a, b } of [{ a: 0, b: 0 }, ...blocks, next]) {
    addEdge("h", b, a); addEdge("h", b + 1, a); addEdge("v", a, b); addEdge("v", a + 1, b);
  }
  const span = (dir, line) => {
    const pos = [...edges.values()].filter((e) => e.dir === dir && e.line === line).map((e) => e.pos);
    return [Math.min(...pos), Math.max(...pos)];
  };
  const [mainW, mainE] = span("h", 1);
  for (let i = mainW; i <= mainE; i++) addEdge("h", 1, i);
  const [, stationS] = span("v", 0);
  for (let j = 0; j <= stationS; j++) addEdge("v", 0, j);
  const lineName = (dir, line) => (dir === "h" ? ALONG[line] : line === 0 ? "Вокзальная улица" : line > 0 ? ACROSS_EAST[line - 1] : ACROSS_WEST[-line - 1]);
  const edgeEnds = (e) => (e.dir === "h" ? [at(e.pos * BU, e.line * BV), at((e.pos + 1) * BU, e.line * BV)] : [at(e.line * BU, e.pos * BV), at(e.line * BU, (e.pos + 1) * BV)]);
  const halfOf = (dir, line, pos) => { const e = edges.get(`${dir}:${line}:${pos}`); return e ? hw(e.cls) : 0; };

  // Занятые места: прямоугольник — середина, направление вдоль, полуразмеры.
  const taken = [];
  const rectOf = (c, t, along, across) => ({ c, t, n: rightOf(t), a: along / 2, b: across / 2 });
  const clashes = (r) => taken.some((q) => overlap(r, q));
  const dryRect = (r, margin = 0.2) => corners(r).concat([r.c]).every((p) => inMap(p, 1) && !water(p[0], p[1], margin));
  const roadRect = (p, q, width) => rectOf([(p[0] + q[0]) / 2, (p[1] + q[1]) / 2], unit([q[0] - p[0], q[1] - p[1]]), dist(p, q) + width, width + m(2));

  const ways = [];      // улицы и дороги: { tags, pts }
  const reserve = (tags, pts) => {
    const width = m(config.roads[tags.highway].width);
    for (let i = 0; i + 1 < pts.length; i++) if (dist(pts[i], pts[i + 1]) > 0.01) taken.push({ ...roadRect(pts[i], pts[i + 1], width), road: tags.name || true });
  };
  const roadWay = (tags, pts) => {
    ways.push({ tags, pts });
    reserve(tags, pts);
  };
  // В 1930-х асфальта в уездном городке нет: все улицы и тракты грунтовые.
  const streetTags = (cls, nm) => ({ highway: cls, name: nm, surface: "unpaved" });

  // Сетка улиц: соседние стороны одного ряда и вида — один путь.
  const groups = new Map();
  for (const e of edges.values()) {
    const g = `${e.dir}:${e.line}:${e.cls}`;
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g).push(e);
  }
  for (const list of groups.values()) {
    list.sort((p, q) => p.pos - q.pos);
    let run = [list[0]];
    const flush = () => {
      const pts = [edgeEnds(run[0])[0], edgeEnds(run[run.length - 1])[1]];
      roadWay(streetTags(run[0].cls, lineName(run[0].dir, run[0].line)), pts);
    };
    for (let k = 1; k < list.length; k++) {
      if (list[k].pos === run[run.length - 1].pos + 1) run.push(list[k]);
      else { flush(); run = [list[k]]; }
    }
    flush();
  }

  // Тракты: от концов Соборной и Вокзальной к краю карты; вдоль Волги — не ближе RIVER_KEEP к берегу. Путь
  // чертится до края, чтобы было где искать места окраин, а на карту ложится только до последнего, что на нём
  // встало: дальше в зародыше ехать некуда.
  const tractPath = (start, dir, goal, keep) => {
    const pts = [start];
    let p = start, d = dir;
    for (let k = 0; k < 200; k++) {
      d = unit([d[0] * 0.85 + goal[0] * 0.15, d[1] * 0.85 + goal[1] * 0.15]);
      const q = add(p, d, 4);
      if (keep) q[1] = Math.max(q[1], shore(q[0]) + m(keep));
      pts.push(q);
      p = q;
      if (!inMap(q, -2)) break;
    }
    return pts;
  };
  // Куски пути по суше и над водой: над водой — мост.
  const splitByWater = (pts, margin) => {
    const pieces = [];
    let cur = { bridge: water(pts[0][0], pts[0][1], margin), pts: [pts[0]] };
    for (let i = 0; i + 1 < pts.length; i++) {
      const n = Math.max(1, Math.ceil(dist(pts[i], pts[i + 1]) / 0.25));
      for (let k = 1; k <= n; k++) {
        const p = [pts[i][0] + ((pts[i + 1][0] - pts[i][0]) * k) / n, pts[i][1] + ((pts[i + 1][1] - pts[i][1]) * k) / n];
        const wet = water(p[0], p[1], margin);
        if (wet !== cur.bridge) {
          cur.pts.push(p);
          pieces.push(cur);
          cur = { bridge: wet, pts: [p] };
        }
      }
      cur.pts.push(pts[i + 1]);
    }
    pieces.push(cur);
    return pieces.filter((piece) => piece.pts.length >= 2);
  };
  const tractPieces = (pts, tags) => splitByWater(pts, m(BRIDGE_MARGIN)).map((piece) => ({ tags: piece.bridge ? { ...tags, bridge: "yes" } : tags, pts: piece.pts }));
  const tractWay = (pts, tags) => {
    for (const piece of tractPieces(pts, tags)) roadWay(piece.tags, piece.pts);
  };
  const west = tractPath(at(mainW * BU, BV), [-U[0], -U[1]], [-1, 0], RIVER_KEEP);
  const east = tractPath(at((mainE + 1) * BU, BV), U, [1, 0], RIVER_KEEP);
  const southAll = tractPath(at(0, (stationS + 1) * BV), V, [0, 1], 0);
  const westTags = streetTags("secondary", tracts[0] + " тракт"), eastTags = streetTags("secondary", tracts[1] + " тракт");
  const westName = westTags.name, eastName = eastTags.name;
  for (const piece of [...tractPieces(west, westTags), ...tractPieces(east, eastTags)]) reserve(piece.tags, piece.pts);
  // Докуда тракт нужен: дальний край последнего, что на нём встало.
  const reach = new Map([[west, 0], [east, 0]]);
  const reached = (path, s) => { if (reach.has(path)) reach.set(path, Math.max(reach.get(path), s)); };

  // Станция — где Вокзальная улица пересекает ветку; ветка идёт с запада и кончается у завода.
  const corner = blocks.concat([next]).flatMap(({ a, b }) => [at(a * BU, (b + 1) * BV), at((a + 1) * BU, (b + 1) * BV)]);
  const townSouth = Math.max(...corner.map((p) => p[1]));
  const townWest = Math.min(...corner.map((p) => p[0])), townEast = Math.max(...corner.map((p) => p[0]));
  const railWave = wave(m(60), m(2400));
  const railY0 = Math.min(townSouth + m(560), H - m(420));
  const rail = (x) => railY0 + railWave(x);
  const cross = crossing(southAll, (p) => p[1] - rail(p[0]));
  if (!cross) throw new Error("Вокзальная улица не дошла до железной дороги");
  const streetAt = crossing(southAll, (p) => p[1] - (rail(p[0]) - m(STATION_STREET)));
  if (!streetAt) throw new Error("Вокзальная улица не дошла до Станционной");
  let railEnd = cross.point[0];
  while (railEnd < cross.point[0] + m(520) && !water(railEnd + 0.5, rail(railEnd + 0.5), m(60))) railEnd += 0.5;
  // Вокзальная кончается у путей: за станцией ехать некуда.
  const [toRail] = cutAt(southAll, cross.index, cross.point);
  tractWay(headOf(toRail, pathLength(toRail) - hw("secondary") - m(config.rail.width)), streetTags("secondary", "Вокзальная улица"));
  const railPts = [];
  for (let x = -2; x < railEnd; x += 4) railPts.push([x, rail(x)]);
  railPts.push([railEnd, rail(railEnd)]);
  for (let i = 0; i + 1 < railPts.length; i++) taken.push(roadRect(railPts[i], railPts[i + 1], m(config.rail.width)));
  const stationPts = [];
  for (let x = streetAt.point[0]; x < railEnd - m(20); x += 4) stationPts.push([x, rail(x) - m(STATION_STREET)]);
  stationPts.push([railEnd - m(20), rail(railEnd - m(20)) - m(STATION_STREET)]);
  roadWay(streetTags("residential", "Станционная улица"), stationPts);

  // Здания и площадки.
  const buildings = [];  // { tags, ring, id? }
  const pads = [];       // { p, place }
  const building = (r, tags, wet = false) => {
    if ((!wet && !dryRect(r)) || clashes(r)) return null;
    taken.push(r);
    const b = { tags, ring: corners(r) };
    buildings.push(b);
    return b;
  };
  // Дом лицом к улице: `S` — точка на оси улицы, `t` — вдоль улицы, `n` — от улицы вглубь участка.
  const facing = (S, t, n, half, size, tags, shift = 0) => {
    const [front, depth] = size;
    const c = add(add(S, n, half + m(SETBACK) + m(depth) / 2), t, shift);
    return building(rectOf(c, t, m(front), m(depth)), tags);
  };
  const shedBehind = (S, t, n, half, depth, shift) => {
    const c = add(add(S, n, half + m(SETBACK) + m(depth) + m(5) + m(SHED[1]) / 2), t, shift);
    building(rectOf(c, t, m(SHED[0]), m(SHED[1])), { building: "shed" });
  };
  // Площадка вдоль направления `t`: заправка, мастерская.
  const pad = (p, t, size, place) => {
    const r = rectOf(p, t, m(size[0]), m(size[1]));
    if (!dryRect(r) || clashes(r)) return false;
    taken.push(r);
    pads.push({ p, place: { ...place, pad: size, pad_angle: round((((Math.atan2(t[1], t[0]) * 180) / Math.PI) + 360) % 180, 2) } });
    return true;
  };

  // Площадь: собор, торговые ряды, рынок; пристань на берегу перед ней.
  const places = [];
  const placeBuilding = (b, place) => {
    if (!b) throw new Error(`место «${place.id}» не встало на карту`);
    b.place = place;
    places.push(place);
  };
  const must = (b, what) => {
    if (!b) throw new Error(`${what} не встал на карту`);
    return b;
  };
  const inSquare = (u, v, size, tags) => building(rectOf(at(u * BU, v * BV), U, m(size[0]), m(size[1])), tags);
  must(inSquare(0.28, 0.45, [22, 14], { building: "church", name: "Собор" }), "собор");
  must(inSquare(0.5, 0.8, [50, 10], { building: "retail", name: "Торговые ряды" }), "торговые ряды");
  placeBuilding(inSquare(0.72, 0.4, [28, 24], { building: "retail" }), { id: "market", name: "Рынок" });
  let shoreP = at(0.5 * BU, 0);
  while (!water(shoreP[0], shoreP[1])) shoreP = add(shoreP, V, -0.1);
  placeBuilding(building(rectOf(add(shoreP, V, 0.25), U, m(30), m(10)), { building: "industrial" }, true), { id: "pier", name: "Пристань" });

  // Усадьбы кварталов: по длинным сторонам вдоль улиц и по одной в середине коротких.
  const plots = [];
  const squareMid = at(0.5 * BU, 0.5 * BV);
  for (const { a, b } of blocks) {
    const near = Math.max(Math.abs(a + 0.5), b + 0.5) <= 1.6;
    for (const side of ["n", "s"]) {
      const j = side === "n" ? b : b + 1;
      const half = halfOf("h", j, a);
      const u0 = a * BU + halfOf("v", a, b) + m(2), u1 = (a + 1) * BU - halfOf("v", a + 1, b) - m(2);
      const count = Math.max(1, Math.round((u1 - u0) / m(PLOT)));
      const step = (u1 - u0) / count;
      for (let k = 0; k < count; k++) {
        const u = u0 + (k + 0.5) * step;
        // Каменные дома — на главных улицах вблизи площади, и то не сплошь.
        const stone = j <= 1 && dist(at(u, j * BV), squareMid) <= m(STONE_REACH) && rand() < 0.75;
        plots.push({ street: ALONG[j], along: u, odd: side === "s", S: at(u, j * BV), t: U, n: side === "n" ? V : [-V[0], -V[1]], half, room: step, stone, near });
      }
    }
    for (const side of ["w", "e"]) {
      const i = side === "w" ? a : a + 1;
      const half = halfOf("v", i, b);
      const v = (b + 0.5) * BV;
      plots.push({ street: lineName("v", i), along: v, odd: side === "e", S: at(i * BU, v), t: V, n: side === "w" ? U : [-U[0], -U[1]], half, room: m(BLOCK_V) - 2 * m(CORNER), stone: false, near });
    }
  }
  // Слобода вдоль западного тракта за городом.
  const westLen = pathLength(west);
  for (let s = m(200); s < Math.min(m(620), westLen - 2); s += m(SLOBODA_PLOT)) {
    reached(west, s + m(SLOBODA_PLOT) / 2);
    const { p, t } = pointAt(west, s);
    for (const odd of [true, false]) {
      const n = odd ? [-rightOf(t)[0], -rightOf(t)[1]] : rightOf(t);
      plots.push({ street: westName, along: s, odd, S: p, t, n, half: hw("secondary"), room: m(SLOBODA_PLOT), stone: false, near: false });
    }
  }

  // Места на усадьбах: контора — ближе всех к площади, магазины и артели — по краям и в слободе.
  const used = new Set();
  const nearest = (target, test = () => true) => {
    let best = null;
    for (const pl of plots) {
      if (used.has(pl) || pl.stone || !test(pl)) continue;
      const d = dist(pl.S, target);
      if (!best || d < best.d) best = { pl, d };
    }
    if (!best) throw new Error("не нашлось усадьбы для места");
    used.add(best.pl);
    return best.pl;
  };
  const onPlot = (pl, size, tags, place) => placeBuilding(facing(pl.S, pl.t, pl.n, pl.half, size, tags), place);
  const inTown = (pl) => pl.street !== westName;
  onPlot(nearest(at(0.5 * BU, 1.2 * BV), inTown), [12, 8], { building: "house" }, { id: "office", name: "Контора", color: "#e3c27a" });
  onPlot(nearest(at(1.5 * BU, 1.5 * BV), inTown), STONE, { building: "retail" }, { id: "jewelry", name: "Ювелирная артель" });
  onPlot(nearest(at((mainW - 1) * BU, 1.5 * BV), inTown), [10, 8], { building: "retail" }, { id: "shop_1", name: "Булочная № 1" });
  onPlot(nearest(at((mainE + 1) * BU, 1.5 * BV), inTown), [10, 8], { building: "retail" }, { id: "shop_2", name: "Магазин № 2" });
  const sloboda = (pl) => pl.street === westName;
  onPlot(nearest(pointAt(west, m(330)).p, sloboda), [18, 10], { building: "industrial" }, { id: "furniture", name: "Мебельная артель", height: 5 });
  onPlot(nearest(pointAt(west, m(470)).p, sloboda), [10, 8], { building: "retail" }, { id: "shop_3", name: "Лавка № 3" });

  // Дома на остальных усадьбах: у площади каменные, в центре чаще двухэтажные, на окраинах избы.
  for (const pl of plots) {
    if (used.has(pl)) continue;
    if (pl.stone) {
      facing(pl.S, pl.t, pl.n, pl.half, STONE, { building: "retail" });
      continue;
    }
    const big = rand() < (pl.near ? 0.4 : 0.12);
    const size = big ? SQUARE_HOUSE : IZBA;
    const play = Math.max(0, pl.room - m(size[0]) - m(4));
    const shift = (rand() - 0.5) * play;
    const house = facing(pl.S, pl.t, pl.n, pl.half, size, { building: "house" }, shift);
    if (!house) continue;
    pl.house = house;
    if (rand() < 0.65) shedBehind(pl.S, pl.t, pl.n, pl.half, size[1], shift + (rand() < 0.5 ? -1 : 1) * m(size[0] / 2));
  }
  // Город дальше — только кварталами: окраинам нельзя заходить в кварталы и на площадь.
  for (const blk of [{ a: 0, b: 0 }, ...blocks, next]) taken.push(rectOf(at((blk.a + 0.5) * BU, (blk.b + 0.5) * BV), U, BU, BV));
  // Номера домов: по улице, нечётные по одной стороне, чётные по другой.
  const byStreet = new Map();
  for (const pl of plots) {
    if (!byStreet.has(pl.street)) byStreet.set(pl.street, []);
    byStreet.get(pl.street).push(pl);
  }
  for (const list of byStreet.values()) {
    for (const odd of [true, false]) {
      const side = list.filter((pl) => pl.odd === odd).sort((p, q) => p.along - q.along);
      side.forEach((pl, k) => {
        if (!pl.house) return;
        pl.house.tags["addr:street"] = pl.street;
        pl.house.tags["addr:housenumber"] = String(odd ? 2 * k + 1 : 2 * k + 2);
      });
    }
  }

  // Окраины: склад стройматериалов на берегу у западного тракта, бензоколонка, швейная фабрика у притока.
  // Здание или площадка у дороги `path` с полушириной `half`: первое из мест `[s, справа ли]`, где встало.
  const sideOf = (t, right) => (right ? rightOf(t) : [-rightOf(t)[0], -rightOf(t)[1]]);
  const alongRoad = (path, half, list, size, tags, place) => {
    for (const [s, right] of list) {
      if (s < 0 || s > pathLength(path) - 1) continue;
      const { p, t } = pointAt(path, s);
      const b = facing(p, t, sideOf(t, right), half, size, tags);
      if (b) {
        if (place) placeBuilding(b, place);
        reached(path, s + m(size[0]) / 2);
        return b;
      }
    }
    if (place) throw new Error(`место «${place.id}» не встало у дороги — смени town.seed`);
    return null;
  };
  const padAlongRoad = (path, half, list, size, place) => {
    for (const [s, right] of list) {
      if (s < 0 || s > pathLength(path) - 1) continue;
      const { p, t } = pointAt(path, s);
      if (pad(add(p, sideOf(t, right), half + m(SETBACK) + m(size[1]) / 2), t, size, place)) {
        reached(path, s + m(size[0]) / 2);
        return;
      }
    }
    throw new Error(`площадка «${place.id}» не встала у дороги — смени town.seed`);
  };
  const both = (list, first) => list.flatMap((s) => [[m(s), first], [m(s), !first]]);
  const tryAlong = (path, list, size, tags, place) => alongRoad(path, hw("secondary"), list, size, tags, place);
  const riverSide = (path, s) => { const { t } = pointAt(path, s); return rightOf(t)[1] < 0; };
  tryAlong(west, [m(110), m(150), m(80)].flatMap((s) => [[s, riverSide(west, s)], [s, !riverSide(west, s)]]), [30, 18], { building: "industrial" }, { id: "materials", name: "Склад стройматериалов" });
  const fuel = { color: "#8d959c" };
  padAlongRoad(west, hw("secondary"), both([40, 60, 90, 130, 170, 25], !riverSide(west, m(40))), [30, 22], { id: "fuel_1", name: "Бензоколонка № 1", ...fuel });
  const bridgeAt = splitByWater(east, m(BRIDGE_MARGIN)).findIndex((piece) => piece.bridge);
  const beforeBridge = bridgeAt > 0 ? pathLength(splitByWater(east, m(BRIDGE_MARGIN))[0].pts) : pathLength(east);
  tryAlong(east, [beforeBridge - m(90), beforeBridge - m(150), beforeBridge + m(140), beforeBridge + m(220)].flatMap((s) => [[s, true], [s, false]]), [60, 20], { building: "industrial" }, { id: "sewing", name: "Швейная фабрика" });
  tractWay(headOf(west, reach.get(west) + m(TRACT_TAIL)), westTags);
  tractWay(headOf(east, reach.get(east) + m(TRACT_TAIL)), eastTags);

  // Станция: вокзал у конца Вокзальной улицы, товарный двор, база и завод между Станционной улицей и путями, дома рабочих напротив.
  const station = (list, right, size, tags, place) => alongRoad(stationPts, hw("residential"), list.map((s) => [m(s), right]), size, tags, place);
  const railT = (x) => unit([1, rail(x + 0.5) - rail(x - 0.5)]);
  const depotC = [cross.point[0] - m(40), rail(cross.point[0] - m(40)) - m(16)];
  must(building(rectOf(depotC, railT(depotC[0]), m(20), m(10)), { building: "train_station", name: "Вокзал" }), "вокзал");
  station([70, 50, 95], true, [40, 14], { building: "industrial" }, { id: "station", name: "Товарный двор станции" });
  station([150, 130, 175], true, [30, 18], { building: "industrial" }, { id: "wholesale", name: "Оптовая база" });
  station([270, 250, 300, 225], true, [70, 24], { building: "industrial" }, { id: "plant", name: "Механический завод" });
  station([345, 370], true, [36, 18], { building: "industrial" });
  station([60, 75, 90], false, [10, 8], { building: "retail" }, { id: "shop_4", name: "Лавка № 4" });
  const north = (list) => list.map((s) => [m(s), false]), south = (list) => list.map((s) => [m(s), true]);
  padAlongRoad(stationPts, hw("residential"), [...north([30, 45, 110, 140, 200]), ...south([25, 110, 210, 400])], [30, 22], { id: "fuel_2", name: "Бензоколонка № 2", ...fuel });
  padAlongRoad(stationPts, hw("residential"), [...north([330, 360, 300, 240, 400]), ...south([400, 420, 210])], [26, 18], { id: "service_1", name: "Мастерская", color: "#9c9182" });
  let number = 1;
  for (let s = m(100); s < Math.min(m(300), pathLength(stationPts) - m(20)); s += m(SLOBODA_PLOT)) {
    const { p, t } = pointAt(stationPts, s);
    const north = [-rightOf(t)[0], -rightOf(t)[1]];
    const house = facing(p, t, north, hw("residential"), IZBA, { building: "house", "addr:street": "Станционная улица", "addr:housenumber": String(number) });
    if (!house) continue;
    number += 2;
    shedBehind(p, t, north, hw("residential"), IZBA[1], m(IZBA[0] / 2));
  }

  // Хлебозавод — модель своего участка у Вокзальной улицы между городом и станцией: проезд двора выходит на улицу.
  // Участок — тот же, что у хлебозавода Кинешмы: многоугольник, проезды и рампа из её настроек.
  const bakery = config.places?.find((p) => p.id === "bakery")?.territory;
  if (!bakery) throw new Error("в настройках base нет хлебозавода с участком — его участок берётся оттуда");
  const yardRoad = bakery.roads.find((r) => r.hidden), gateRoad = bakery.roads.find((r) => !r.hidden);
  const model = config.models.places.bakery;
  const [yardU, yardV] = yardRoad.path[0];
  const bakeryAt = crossing(southAll, (p) => p[1] - (townSouth + m(230)));
  if (!bakeryAt) throw new Error("хлебозаводу нет места на Вокзальной улице");
  const P = bakeryAt.point;
  const roadT = unit([southAll[bakeryAt.index + 1][0] - southAll[bakeryAt.index][0], southAll[bakeryAt.index + 1][1] - southAll[bakeryAt.index][1]]);
  const toRoad = [-rightOf(roadT)[0], -rightOf(roadT)[1]];   // слева по ходу на юг — восток, к улице
  const rotation = round(((Math.atan2(-toRoad[1], -toRoad[0]) * 180) / Math.PI + 360) % 360, 2);
  const origin = add(P, modelFrame([0, 0], rotation, cell)([yardU, yardV]), -1);
  const atModel = modelFrame(origin, rotation, cell);
  const rad = (rotation * Math.PI) / 180;
  const lot = rectOf(atModel(model.center), [Math.cos(rad), Math.sin(rad)], m(model.size[0]), m(model.size[1]));
  // Участок примыкает к Вокзальной улице — с ней он и соприкасается, с остальным нет.
  const axisInside = southAll.some((a, i) => i + 1 < southAll.length && [0, 0.25, 0.5, 0.75].some((k) => {
    const q = [a[0] + (southAll[i + 1][0] - a[0]) * k, a[1] + (southAll[i + 1][1] - a[1]) * k];
    return overlap(rectOf(q, [1, 0], 0.01, 0.01), lot);
  }));
  if (!dryRect(lot) || axisInside || taken.some((q) => q.road !== "Вокзальная улица" && overlap(lot, q))) throw new Error("участок хлебозавода задевает город, воду или ось улицы — смени town.seed");
  taken.push(lot);
  const [lat, lon] = proj.toLatLon(origin[0], origin[1]);
  places.push({
    id: "bakery",
    name: "Хлебозавод",
    street: "Вокзальная улица",
    lat,
    lon,
    territory: { rotation, clear: bakery.clear, roads: [yardRoad, gateRoad], stop: bakery.stop },
  });
  // Ворота с другой стороны участка соединены с улицей проездом.
  const gate = atModel(gateRoad.path[0]);
  const gateTo = nearestOnPath(southAll, gate);
  const drive = roadRect(gate, gateTo, m(config.roads.service.width));
  if (taken.some((q) => q.road !== "Вокзальная улица" && overlap(drive, q))) throw new Error("проезд к воротам хлебозавода задевает другое");
  ways.push({ tags: { highway: "service", surface: "unpaved" }, pts: [gate, gateTo] });

  // Стройка в размеченном квартале.
  const siteC = at((next.a + 0.5) * BU, (next.b + 0.5) * BV);
  const siteR = rectOf(siteC, U, m(32), m(24));
  taken.push(siteR);
  const siteWay = { tags: { landuse: "construction" }, ring: corners(siteR) };

  // Земля: город, площадь, размеченный квартал, кладбище, луга вдоль воды, поля полосами и лес.
  const zones = [];
  const blockRing = ({ a, b }) => [at(a * BU, b * BV), at((a + 1) * BU, b * BV), at((a + 1) * BU, (b + 1) * BV), at(a * BU, (b + 1) * BV)];
  for (const blk of blocks) zones.push({ tags: { landuse: "residential" }, ring: blockRing(blk) });
  zones.push({ tags: { landuse: "retail" }, ring: blockRing({ a: 0, b: 0 }) });
  zones.push({ tags: { landuse: "construction" }, ring: blockRing(next) });
  const townRect = rectOf([(townWest + townEast) / 2, (O[1] + townSouth) / 2], [1, 0], townEast - townWest + m(200), townSouth - O[1] + m(200));
  for (const s of [m(160), m(240), m(120)]) {
    const { p, t } = pointAt(west, s);
    const south = rightOf(t)[1] > 0 ? rightOf(t) : [-rightOf(t)[0], -rightOf(t)[1]];
    const r = rectOf(add(p, south, m(110)), t, m(160), m(110));
    if (!dryRect(r) || clashes(r)) continue;
    taken.push(r);
    zones.push({ tags: { landuse: "cemetery" }, ring: corners(r) });
    break;
  }
  const strip = (x0, x1, from, to) => {
    const top = [], bottom = [];
    for (let x = x0; x <= x1; x += 2) { top.push([x, from(x)]); bottom.push([x, to(x)]); }
    return top.concat(bottom.reverse());
  };
  zones.push({ tags: { landuse: "meadow" }, ring: strip(-2, townWest - m(80), (x) => shore(x) - 1, (x) => shore(x) + m(90)) });
  zones.push({ tags: { landuse: "meadow" }, ring: strip(townEast + m(80), W + 2, (x) => shore(x) - 1, (x) => shore(x) + m(90)) });
  const banks = [];
  for (let y = townSouth + m(60); y <= H + 2; y += 2) banks.push(y);
  zones.push({ tags: { landuse: "meadow" }, ring: banks.map((y) => [trib(y) - tribHalf(y) - m(80), y]).concat(banks.slice().reverse().map((y) => [trib(y) + tribHalf(y) + m(80), y])) });
  const center = at(0.5 * BU, BV);
  const randomLand = (minD, maxD) => {
    for (let k = 0; k < 40; k++) {
      const p = [rand() * W, rand() * H];
      const d = dist(p, center);
      if (d >= m(minD) && d <= m(maxD) && !water(p[0], p[1], m(60)) && !overlap(rectOf(p, [1, 0], 1, 1), townRect)) return p;
    }
    return null;
  };
  // Поля: у города мельче, дальше крупнее.
  for (let k = 0; k < 26; k++) {
    const p = randomLand(k < 12 ? 250 : 700, k < 12 ? 1100 : 3200);
    if (!p) continue;
    const big = k < 12 ? 0.6 : 1;
    const along = m((360 + rand() * 240) * big), across = m((200 + rand() * 160) * big);
    const patch = rectOf(p, U, along, across);
    const dry = [-1, -0.5, 0, 0.5, 1].every((i) => [-1, -0.5, 0, 0.5, 1].every((j) => {
      const q = add(add(p, U, (i * along) / 2), V, (j * across) / 2);
      return !water(q[0], q[1], 1);
    }));
    if (!dry || overlap(patch, townRect) || taken.some((q) => !q.road && overlap(patch, q))) continue;
    let v = -across / 2;
    while (v < across / 2 - 0.5) {
      const w = Math.min(across / 2 - v, m(40 + rand() * 30));
      const c = add(p, V, v + w / 2);
      zones.push({ tags: { landuse: rand() < 0.35 ? "meadow" : "farmland" }, ring: corners(rectOf(c, U, along, w)) });
      v += w;
    }
  }
  const blob = (p, radius) => {
    const phases = [rand(), rand(), rand()].map((r) => r * Math.PI * 2);
    const ring = [];
    for (let k = 0; k < 28; k++) {
      const f = (k / 28) * Math.PI * 2;
      const r = radius * (1 + 0.16 * Math.sin(2 * f + phases[0]) + 0.09 * Math.sin(3 * f + phases[1]) + 0.05 * Math.sin(7 * f + phases[2]));
      ring.push([p[0] + Math.cos(f) * r, p[1] + Math.sin(f) * r]);
    }
    return ring;
  };
  for (let k = 0; k < 11; k++) {
    const p = randomLand(900, 4000);
    if (p) zones.push({ tags: { natural: "wood" }, ring: blob(p, m(160 + rand() * 260)) });
  }
  for (let k = 0; k < 6; k++) {
    const p = randomLand(500, 3000);
    if (p) zones.push({ tags: { natural: "scrub" }, ring: blob(p, m(60 + rand() * 80)) });
  }

  // Вода: Волга до северного края и приток с юга, заходящий в неё.
  const volga = [[-5, -5], [W + 5, -5]];
  for (let x = W + 5; x >= -5; x -= 2) volga.push([x, shore(x)]);
  const tribY = [];
  for (let y = mouth - m(120); y <= H + 5; y += 1) tribY.push(y);
  const tributary = tribY.map((y) => [trib(y) - tribHalf(y), y]).concat(tribY.slice().reverse().map((y) => [trib(y) + tribHalf(y), y]));

  // Элементы выгрузки: широта и долгота по рамке настроек.
  const elements = [];
  let nextId = 1;
  const ll = ([x, y]) => { const [la, lo] = proj.toLatLon(x, y); return { lat: la, lon: lo }; };
  const way = (tags, pts, closed) => {
    const geometry = pts.map(ll);
    if (closed) geometry.push({ ...geometry[0] });
    const el = { type: "way", id: nextId++, tags, geometry };
    elements.push(el);
    return el;
  };
  way({ natural: "water" }, volga, true);
  way({ natural: "water", name: "приток" }, tributary, true);
  for (const z of zones) way(z.tags, z.ring, true);
  way({ railway: "rail" }, railPts, false);
  for (const w of ways) way(w.tags, w.pts, false);
  for (const b of buildings) {
    const el = way(b.tags, b.ring, true);
    if (b.place) b.place.osm = "w" + el.id;
  }
  const site = way(siteWay.tags, siteWay.ring, true);
  places.push({ id: "site", name: "Стройка", osm: "w" + site.id, site: true, color: "#b79e74" });
  for (const pd of pads) {
    const [la, lo] = proj.toLatLon(pd.p[0], pd.p[1]);
    const el = { type: "node", id: nextId++, lat: la, lon: lo, tags: {} };
    elements.push(el);
    pd.place.osm = "n" + el.id;
    places.push(pd.place);
  }
  // Значок места — его имя без номера: у всех магазинов значок магазина, у бензоколонок — колонки.
  for (const p of places) p.icon = p.id.replace(/_\d+$/, "");
  return { name, elements, places, stats: { blocks: blocks.length, buildings: buildings.length } };
}

/** Перемешанная копия списка. */
function shuffled(list, rand) {
  const out = list.slice();
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

function corners(r) {
  return [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([i, j]) => [r.c[0] + r.t[0] * r.a * i + r.n[0] * r.b * j, r.c[1] + r.t[1] * r.a * i + r.n[1] * r.b * j]);
}

/** Пересекаются ли два повёрнутых прямоугольника (разделяющая ось). */
function overlap(p, q) {
  for (const axis of [p.t, p.n, q.t, q.n]) {
    const extent = (r) => {
      const c = r.c[0] * axis[0] + r.c[1] * axis[1];
      const e = r.a * Math.abs(r.t[0] * axis[0] + r.t[1] * axis[1]) + r.b * Math.abs(r.n[0] * axis[0] + r.n[1] * axis[1]);
      return [c - e, c + e];
    };
    const [a0, a1] = extent(p), [b0, b1] = extent(q);
    if (a1 <= b0 || b1 <= a0) return false;
  }
  return true;
}

function pathLength(pts) {
  let s = 0;
  for (let i = 0; i + 1 < pts.length; i++) s += dist(pts[i], pts[i + 1]);
  return s;
}

/** Точка ломаной на расстоянии `s` от начала и направление в ней. */
function pointAt(pts, s) {
  for (let i = 0; i + 1 < pts.length; i++) {
    const len = dist(pts[i], pts[i + 1]);
    if (s <= len || i + 2 === pts.length) {
      const t = unit([pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]]);
      return { p: add(pts[i], t, Math.min(s, len)), t };
    }
    s -= len;
  }
  throw new Error("пустая ломаная");
}

/** Первое место ломаной, где `f` меняет знак: номер куска и точка. */
function crossing(pts, f) {
  for (let i = 0; i + 1 < pts.length; i++) {
    const fa = f(pts[i]), fb = f(pts[i + 1]);
    if (fa === 0) return { index: i, point: pts[i] };
    if ((fa < 0) !== (fb < 0)) {
      const k = fa / (fa - fb);
      return { index: i, point: [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * k, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * k] };
    }
  }
  return null;
}

/** Начало ломаной длиной `s`. */
function headOf(pts, s) {
  const out = [pts[0]];
  for (let i = 0; i + 1 < pts.length; i++) {
    const len = dist(pts[i], pts[i + 1]);
    if (s <= 1e-9) return out;  // длина кончилась ровно на вершине
    if (s <= len) {
      out.push(add(pts[i], unit([pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]]), s));
      return out;
    }
    out.push(pts[i + 1]);
    s -= len;
  }
  return out;
}

/** Ломаная, разрезанная в точке `point` куска `index`. */
function cutAt(pts, index, point) {
  return [pts.slice(0, index + 1).concat([point]), [point].concat(pts.slice(index + 1))];
}

/** Ближайшая к `p` точка ломаной. */
function nearestOnPath(pts, p) {
  let best = null;
  for (let i = 0; i + 1 < pts.length; i++) {
    const n = nearestOnSegment(p, pts[i], pts[i + 1]);
    if (!best || n.distance < best.distance) best = n;
  }
  return best.point;
}

/** Настройки зародыша: поправки файла поверх настроек `base` (путь — от файла зародыша); списки заменяются целиком. */
export async function readSeedConfig(path) {
  const own = JSON.parse(await readFile(path, "utf8"));
  if (!own.base) return own;
  const base = JSON.parse(await readFile(resolve(dirname(path), own.base), "utf8"));
  return merged(base, own);
}

function merged(base, over) {
  if (!isObject(base) || !isObject(over)) return over;
  const out = { ...base };
  for (const [k, v] of Object.entries(over)) out[k] = k in base ? merged(base[k], v) : v;
  return out;
}

function isObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

export async function run(configPath, root = process.cwd()) {
  const config = await readSeedConfig(configPath);
  const plan = planTown(config);
  const cityConfig = { ...config, places: plan.places };
  const city = buildCity({ elements: plan.elements }, cityConfig);
  const stats = await writeCity(city, cityConfig, plan.name, root);
  return { town: plan.name, ...plan.stats, ...stats };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const configPath = process.argv[2];
  if (!configPath) {
    console.error("укажи файл настроек зародыша: node tools/city/seed.mjs art/transport/seed.json");
    process.exit(2);
  }
  run(resolve(configPath)).then(
    (stats) => console.log(JSON.stringify(stats)),
    (error) => {
      console.error(error.message);
      process.exit(1);
    },
  );
}
