// Зародыш города Транспортной компании («Фаза-04-зародыш-города»): программа чертит уездный городок по
// регулярному плану и отдаёт его переводу карты тем же языком, что выгрузка OpenStreetMap. Волга идёт вдоль
// северного края, с юга в неё впадает приток. Городок — четыре квартала у берега: площадь с собором, торговыми
// рядами и рынком и кварталы усадеб `town.blocks` вокруг неё, по кварталу — усадьбы: дом лицом к улице, во дворе
// сарай. За кварталами построек и дорог нет — луга, поля и лес. Всё считается из `town.seed`: одни и те же
// настройки дают ту же карту.
// Настройки зародыша — поправки поверх настроек Кинешмы (`base`): тот же вид улиц, домов, деревьев и моделей.
// Запуск: node tools/city/seed.mjs art/transport/seed.json

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { buildCity } from "./city.mjs";
import { projector, random } from "./geo.mjs";
import { writeCity } from "./osm2scene.mjs";

// Размеры в метрах.
const BLOCK_U = 120;         // квартал вдоль реки
const BLOCK_V = 100;         // квартал от реки
const PLOT = 22;             // усадьба вдоль улицы
const SETBACK = 3;           // от края улицы до дома
const CORNER = 26;           // угол квартала, занятый усадьбами длинных сторон
const IZBA = [6.5, 8.5];     // изба: по улице × вглубь — щипцом к улице
const SQUARE_HOUSE = [8, 8]; // двухэтажный деревянный или одноэтажный кирпичный дом
const STONE = [14, 10];      // двухэтажный каменный дом у площади
const SHED = [4, 6];
const STONE_REACH = 210;     // каменные дома — не дальше от середины площади

// Ряды плана: `j` — улицы вдоль реки от берега, `i` — поперечные от площади (0 — Вокзальная).
const ALONG = ["Набережная улица", "Соборная улица", "Советская улица", "Садовая улица", "Полевая улица", "Луговая улица", "Огородная улица", "Новая улица", "Лесная улица"];
const ACROSS_WEST = ["Никольская улица", "Пролетарская улица", "Ильинская улица", "Кузнечная улица", "Песочная улица", "Красноармейская улица", "Овражная улица", "Сенная улица", "Конная улица", "Ямская улица"];
const ACROSS_EAST = ["Торговая улица", "Первомайская улица", "Кооперативная улица", "Почтовая улица", "Школьная улица", "Заводская улица", "Колхозная улица", "Церковная улица", "Рыбацкая улица", "Береговая улица"];

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
  const name = town.names[Math.floor(rand() * town.names.length)];
  // Два числа прежде шли на порядок трактов: без них та же `town.seed` дала бы другой берег и наклон улиц.
  rand();
  rand();

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

  // Кварталы: площадь (0, 0) и кварталы усадеб — номера `[a, b]`: `a` вдоль реки, `b` от реки.
  const blockOk = (a, b) => {
    for (let i = 0; i <= 3; i++) {
      for (let k = 0; k <= 3; k++) {
        const p = at((a + i / 3) * BU, (b + k / 3) * BV);
        if (!inMap(p, 3) || water(p[0], p[1], m(30))) return false;
      }
    }
    return true;
  };
  const square = { a: 0, b: 0 };
  const blocks = town.blocks.map(([a, b]) => ({ a, b }));
  for (const { a, b } of [square, ...blocks]) {
    if (!blockOk(a, b)) throw new Error(`квартал ${a}, ${b} в воде или у края карты — смени town.seed`);
  }

  // Улицы: стороны кварталов; Соборная за площадью и Вокзальная вдоль неё — главные, стороны площади шире.
  const hw = (cls) => m(config.roads[cls].width) / 2;
  const edges = new Map();
  const edgeClass = (dir, line, pos) => {
    if ((dir === "h" && line === 1) || (dir === "v" && line === 0)) return "secondary";
    return (line === 0 || line === 1) && pos === 0 ? "tertiary" : "residential";  // стороны площади шире
  };
  const addEdge = (dir, line, pos) => edges.set(`${dir}:${line}:${pos}`, { dir, line, pos, cls: edgeClass(dir, line, pos) });
  for (const { a, b } of [square, ...blocks]) {
    addEdge("h", b, a); addEdge("h", b + 1, a); addEdge("v", a, b); addEdge("v", a + 1, b);
  }
  const lineName = (dir, line) => (dir === "h" ? ALONG[line] : line === 0 ? "Вокзальная улица" : line > 0 ? ACROSS_EAST[line - 1] : ACROSS_WEST[-line - 1]);
  const edgeEnds = (e) => (e.dir === "h" ? [at(e.pos * BU, e.line * BV), at((e.pos + 1) * BU, e.line * BV)] : [at(e.line * BU, e.pos * BV), at(e.line * BU, (e.pos + 1) * BV)]);
  const halfOf = (dir, line, pos) => { const e = edges.get(`${dir}:${line}:${pos}`); return e ? hw(e.cls) : 0; };

  // Занятые места: прямоугольник — середина, направление вдоль, полуразмеры.
  const taken = [];
  const rectOf = (c, t, along, across) => ({ c, t, n: rightOf(t), a: along / 2, b: across / 2 });
  const clashes = (r) => taken.some((q) => overlap(r, q));
  const dryRect = (r, margin = 0.2) => corners(r).concat([r.c]).every((p) => inMap(p, 1) && !water(p[0], p[1], margin));
  const roadRect = (p, q, width) => rectOf([(p[0] + q[0]) / 2, (p[1] + q[1]) / 2], unit([q[0] - p[0], q[1] - p[1]]), dist(p, q) + width, width + m(2));

  const ways = [];      // улицы: { tags, pts }
  const roadWay = (tags, pts) => {
    ways.push({ tags, pts });
    const width = m(config.roads[tags.highway].width);
    for (let i = 0; i + 1 < pts.length; i++) if (dist(pts[i], pts[i + 1]) > 0.01) taken.push({ ...roadRect(pts[i], pts[i + 1], width), road: tags.name || true });
  };
  // В 1930-х асфальта в уездном городке нет: все улицы грунтовые.
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

  // Здания.
  const buildings = [];  // { tags, ring, place? }
  const building = (r, tags) => {
    if (!dryRect(r) || clashes(r)) return null;
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

  // Площадь: собор, торговые ряды, рынок.
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

  // Места на усадьбах: контора — ближе всех к площади, ювелирная артель — к востоку от неё, магазины — по краям.
  const used = new Set();
  const nearest = (target) => {
    let best = null;
    for (const pl of plots) {
      if (used.has(pl) || pl.stone) continue;
      const d = dist(pl.S, target);
      if (!best || d < best.d) best = { pl, d };
    }
    if (!best) throw new Error("не нашлось усадьбы для места");
    used.add(best.pl);
    return best.pl;
  };
  const onPlot = (pl, size, tags, place) => placeBuilding(facing(pl.S, pl.t, pl.n, pl.half, size, tags), place);
  const west = Math.min(square.a, ...blocks.map((blk) => blk.a)), east = Math.max(square.a, ...blocks.map((blk) => blk.a));
  onPlot(nearest(at(0.5 * BU, 1.2 * BV)), [12, 8], { building: "house" }, { id: "office", name: "Контора", color: "#e3c27a" });
  onPlot(nearest(at(1.5 * BU, 1.5 * BV)), STONE, { building: "retail" }, { id: "jewelry", name: "Ювелирная артель" });
  onPlot(nearest(at((west - 1) * BU, 1.5 * BV)), [10, 8], { building: "retail" }, { id: "shop_1", name: "Булочная № 1" });
  onPlot(nearest(at((east + 1) * BU, 1.5 * BV)), [10, 8], { building: "retail" }, { id: "shop_2", name: "Магазин № 2" });

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

  // Земля: кварталы и площадь, луга вдоль воды, поля полосами и лес.
  const zones = [];
  const blockRing = ({ a, b }) => [at(a * BU, b * BV), at((a + 1) * BU, b * BV), at((a + 1) * BU, (b + 1) * BV), at(a * BU, (b + 1) * BV)];
  for (const blk of blocks) zones.push({ tags: { landuse: "residential" }, ring: blockRing(blk) });
  zones.push({ tags: { landuse: "retail" }, ring: blockRing(square) });
  const corner = [square, ...blocks].flatMap(({ a, b }) => [at(a * BU, (b + 1) * BV), at((a + 1) * BU, (b + 1) * BV)]);
  const townSouth = Math.max(...corner.map((p) => p[1]));
  const townWest = Math.min(...corner.map((p) => p[0])), townEast = Math.max(...corner.map((p) => p[0]));
  const townRect = rectOf([(townWest + townEast) / 2, (O[1] + townSouth) / 2], [1, 0], townEast - townWest + m(200), townSouth - O[1] + m(200));
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
  for (const w of ways) way(w.tags, w.pts, false);
  for (const b of buildings) {
    const el = way(b.tags, b.ring, true);
    if (b.place) b.place.osm = "w" + el.id;
  }
  // Значок места — его имя без номера: у всех магазинов значок магазина.
  for (const p of places) p.icon = p.id.replace(/_\d+$/, "");
  return { name, elements, places, stats: { blocks: blocks.length, buildings: buildings.length } };
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
