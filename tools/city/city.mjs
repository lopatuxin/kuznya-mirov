// Перевод выгрузки OpenStreetMap в объекты сцены Кузни: дороги, железная дорога, мосты, дома, деревья,
// вода и цвет земли рельефа, места игры на настоящих зданиях. Всё считается из выгрузки и настроек города;
// одни и те же файлы дают одну и ту же сцену.

import { area, clipSegment, inside, nearestOnSegment, orientedBox, projector, random, rectRecord, round } from "./geo.mjs";
import { createRaster, drawLine, fillPolygon, floodFill, sample } from "./raster.mjs";

const UNPAVED = new Set(["unpaved", "ground", "dirt", "gravel", "compacted", "sand", "grass", "fine_gravel", "earth", "mud"]);
const COLLINEAR_DEGREES = 5;

/** Сцена города по выгрузке `osm` (ответ Overpass с `out geom`) и настройкам `config`. */
export function buildCity(osm, config) {
  const proj = projector(config.bbox, config.cell);
  const meters = (m) => m / config.cell;
  const rand = random(config.seed ?? 1);
  const els = osm.elements;
  const ways = els.filter((e) => e.type === "way" && e.geometry);
  const ringOf = (geometry) => geometry.map((p) => proj.toCell(p.lat, p.lon));
  const inScene = ([x, y]) => x >= 0 && y >= 0 && x <= proj.width && y <= proj.height;

  // Земля: многоугольники видов земли с рамками для быстрого поиска.
  const zones = [];
  for (const w of ways) {
    const t = w.tags || {};
    const kind = zoneKind(t);
    if (!kind || !isClosed(w.geometry)) continue;
    const ring = ringOf(w.geometry);
    zones.push({ kind, ring, box: boundsOf(ring), area: area(ring), id: w.id, name: t.name });
  }
  const zoneAt = (p, kinds) => {
    let best = null;
    for (const z of zones) {
      if (kinds && !kinds.has(z.kind)) continue;
      if (p[0] < z.box[0] || p[0] > z.box[2] || p[1] < z.box[1] || p[1] > z.box[3]) continue;
      if (inside(p, z.ring) && (!best || z.area < best.area)) best = z;
    }
    return best;
  };

  // Дороги.
  const roadObjects = [];
  const deckObjects = [];
  const roadSegments = [];
  for (const w of ways) {
    const t = w.tags || {};
    const style = config.roads[t.highway];
    if (!style || t.area === "yes" || (t.highway === "service" && t.service === "parking_aisle")) continue;
    const width = meters(style.width);
    const unpaved = UNPAVED.has(t.surface);
    const bridge = t.bridge && t.bridge !== "no";
    const speed = unpaved ? Math.min(style.speed ?? config.unpaved_speed, config.unpaved_speed) : style.speed;
    for (const [a, b] of straightPieces(ringOf(w.geometry), meters(1))) {
      const clipped = clipSegment(a, b, proj.width, proj.height);
      if (!clipped) continue;
      const [p, q] = clipped;
      const len = Math.hypot(q[0] - p[0], q[1] - p[1]);
      if (len < 0.02) continue;
      const angle = (Math.atan2(q[1] - p[1], q[0] - p[0]) * 180) / Math.PI;
      const cx = (p[0] + q[0]) / 2, cy = (p[1] + q[1]) / 2;
      const rec = {
        ...(t.name ? { name: t.name } : {}),
        ...rectRecord(cx, cy, len + width, width, norm360(angle)),
        color: unpaved ? config.unpaved_color : style.color,
        layer: style.layer,
        road: true,
        ...(speed != null ? { road_speed: round(meters(speed * 1000) / 60 / config.minute_seconds, 3) } : {}),
        ...(bridge ? { bridge: true } : {}),
        map: true,
      };
      roadObjects.push(rec);
      roadSegments.push({ a: p, b: q, width, bridge, service: t.highway === "service", name: t.name || "" });
      if (bridge) {
        deckObjects.push({
          ...rectRecord(cx, cy, len + width, width + meters(2), norm360(angle)),
          shape: "box",
          height: round(meters(config.deck_thickness)),
          color: config.deck_color,
          deck: true,
          map: true,
        });
      }
    }
  }

  // Железная дорога: полосы насыпи, не дороги для машин.
  const railObjects = [];
  for (const w of ways) {
    const t = w.tags || {};
    if (t.railway !== "rail") continue;
    const width = meters(config.rail.width);
    for (const [a, b] of straightPieces(ringOf(w.geometry), meters(1))) {
      const clipped = clipSegment(a, b, proj.width, proj.height);
      if (!clipped) continue;
      const [p, q] = clipped;
      const len = Math.hypot(q[0] - p[0], q[1] - p[1]);
      if (len < 0.02) continue;
      const angle = (Math.atan2(q[1] - p[1], q[0] - p[0]) * 180) / Math.PI;
      railObjects.push({
        ...rectRecord((p[0] + q[0]) / 2, (p[1] + q[1]) / 2, len + width * 0.5, width, norm360(angle)),
        color: config.rail.color,
        layer: 1,
        map: true,
      });
    }
  }

  // Вода: все берега — преграды; замкнутые контуры заливаются целиком (острова — обратно сушей), открытые
  // берега большой реки, обрезанные рамкой выгрузки, — заливкой от затравок. Близкие концы открытых берегов
  // соединяются: так устье, где берег сменяется линией реки, не пропускает воду на сушу.
  const water = createRaster(proj.width, proj.height, config.water.per);
  const closedOuter = [];
  const closedInner = [];
  const openEnds = [];
  const shoreLine = (geometry, role) => {
    const g = geometry.filter(Boolean);
    if (g.length < 2) return;
    const ring = ringOf(g);
    drawLine(water, ring, 2);
    if (isClosed(g)) (role === "inner" ? closedInner : closedOuter).push(ring);
    else openEnds.push(ring[0], ring[ring.length - 1]);
  };
  const memberWays = new Set();
  for (const r of els.filter((e) => e.type === "relation" && isWaterTags(e.tags))) {
    for (const m of r.members || []) {
      if (!m.geometry) continue;
      memberWays.add(m.ref);
      shoreLine(m.geometry, m.role);
    }
  }
  for (const w of ways) if (isWaterTags(w.tags) && !memberWays.has(w.id)) shoreLine(w.geometry, "outer");
  const gap = config.water.join_gap ?? 3;
  for (let i = 0; i < openEnds.length; i++) {
    for (let j = i + 1; j < openEnds.length; j++) {
      const [a, b] = [openEnds[i], openEnds[j]];
      const dist = Math.hypot(a[0] - b[0], a[1] - b[1]);
      if (dist > 0 && dist <= gap && inScene(a) && inScene(b)) drawLine(water, [a, b], 2);
    }
  }
  for (const ring of closedOuter) fillPolygon(water, [ring], 1);
  for (const ring of closedInner) fillPolygon(water, [ring], 0);
  const limit = (config.water.max_seed_share ?? 0.5) * water.width * water.height;
  for (const [lat, lon] of config.water.seeds) {
    const filled = floodFill(water, proj.toCell(lat, lon), 0, 1);
    if (filled > limit) throw new Error(`затравка воды ${lat}, ${lon} залила ${filled} ячеек — она на суше или берег не замкнут`);
  }
  const shore = [];
  for (let i = 0; i < water.data.length; i++) {
    if (water.data[i] !== 2) continue;
    const c = i % water.width;
    const near = [i - 1, i + 1, i - water.width, i + water.width].some(
      (n) => n >= 0 && n < water.data.length && Math.abs((n % water.width) - c) <= 1 && water.data[n] === 1,
    );
    shore.push([i, near ? 1 : 0]);
  }
  for (const [i, v] of shore) water.data[i] = v;

  // Высоты рельефа: две точки на клетку; где вода — глубина, у дорог и домов — суша.
  const d = 2;
  const cols = proj.width * d + 1, rows = proj.height * d + 1;
  // Доля воды вокруг точки по девяти пробам: берег между точками ложится плавно, без ступенек.
  const wetness = new Float32Array(cols * rows);
  const probe = [-0.3, 0, 0.3];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = c / d, y = r / d;
      let wet = 0;
      for (const ox of probe) for (const oy of probe) wet += sample(water, x + ox, y + oy, 0) === 1 ? 1 : 0;
      const share = wet / 9;
      wetness[r * cols + c] = share * share * (3 - 2 * share);
    }
  }
  const dryAround = (rec, margin) => {
    const [px, py] = rec.position, [sx, sy] = rec.size;
    const cx = px + sx / 2, cy = py + sy / 2;
    const rad = ((rec.rotation || 0) * Math.PI) / 180;
    const cos = Math.cos(rad), sin = Math.sin(rad);
    const hx = sx / 2 + margin, hy = sy / 2 + margin;
    const reach = Math.hypot(hx, hy);
    const c0 = Math.max(0, Math.floor((cx - reach) * d)), c1 = Math.min(cols - 1, Math.ceil((cx + reach) * d));
    const r0 = Math.max(0, Math.floor((cy - reach) * d)), r1 = Math.min(rows - 1, Math.ceil((cy + reach) * d));
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const dx = c / d - cx, dy = r / d - cy;
        const u = dx * cos + dy * sin, v = -dx * sin + dy * cos;
        if (Math.abs(u) <= hx && Math.abs(v) <= hy) wetness[r * cols + c] = 0;
      }
    }
  };
  roadObjects.forEach((rec, i) => { if (!roadSegments[i].bridge) dryAround(rec, 0.3); });

  // Дома.
  const buildingObjects = [];
  const houseSpots = [];
  for (const w of ways) {
    const t = w.tags || {};
    if (!t.building || t.building === "no" || !isClosed(w.geometry)) continue;
    const ring = ringOf(w.geometry);
    const box = orientedBox(ring);
    if (!inScene([box.cx, box.cy])) continue;
    const m2 = area(ring) * config.cell * config.cell;
    if (m2 < 8) continue;
    const zone = zoneAt([box.cx, box.cy]);
    const kind = buildingKind(t, m2, box, zone && zone.kind);
    const style = config.buildings[kind];
    const levels = Number.parseFloat(t["building:levels"]);
    const heightM = Number.isFinite(levels) && style.per_level ? levels * style.per_level + 1 : style.height;
    const palette = style.colors;
    const rec = {
      ...(t.name ? { name: t.name } : {}),
      ...rectRecord(box.cx, box.cy, Math.max(box.length, meters(2)), Math.max(box.width, meters(2)), box.angle),
      shape: "box",
      height: round(meters(heightM)),
      color: palette[Math.floor(rand() * palette.length)],
      map: true,
    };
    const address = t["addr:street"] && t["addr:housenumber"] ? t["addr:street"] + ", " + t["addr:housenumber"] : null;
    buildingObjects.push({ rec, osm: "w" + w.id, ring, kind, box, address });
    if (kind === "house") houseSpots.push(box);
    dryAround(rec, 0.2);
  }

  const heights = [];
  for (let r = 0; r < rows; r++) {
    const row = new Array(cols);
    for (let c = 0; c < cols; c++) row[c] = round(config.water.depth * wetness[r * cols + c], 3);
    heights.push(row);
  }

  // Места игры на настоящих зданиях; остановка — на улице главной связной сети, куда доезжает машина.
  const drivable = roadSegments.filter((s) => !s.service && !s.bridge);
  const connected = roadSegments.filter((s) => !s.service);
  const main = mainNetwork(connected, segmentIndex(connected, 4));
  const placeObjects = [];
  for (const place of config.places) {
    const target = findPlace(place, els, buildingObjects, proj, zones, ringOf);
    if (!target) throw new Error(`место «${place.id}» не найдено в выгрузке`);
    const center = target.center;
    let best = null;
    for (const s of drivable) {
      if (!main.has(s)) continue;
      const n = nearestOnSegment(center, s.a, s.b);
      if (!best || n.distance < best.distance) best = { ...n, name: s.name };
    }
    if (!best) throw new Error(`у места «${place.id}» нет проезжей улицы в главной сети`);
    const street = streetForm(best ? best.name : "");
    const name = street ? place.name.replace("{street}", street) : place.name.replace(/ на {street}/, "");
    const fields = { name, place: place.id, stop_x: round(best.point[0]), stop_y: round(best.point[1]) };
    if (target.building) {
      Object.assign(target.building.rec, fields, config.place_fields || {});
      if (place.color) target.building.rec.color = place.color;
      if (place.height) target.building.rec.height = round(meters(place.height));
    } else {
      const box = target.box;
      placeObjects.push({
        ...rectRecord(box.cx, box.cy, box.length, box.width, box.angle),
        color: place.color || "#b9a27a",
        layer: 0,
        ...fields,
        ...(config.place_fields || {}),
        map: true,
      });
    }
  }

  // Занятые места: деревья не встают на дороги, дома и в воду.
  const busy = createRaster(proj.width, proj.height, 4);
  const mark = (rec, margin) => {
    const [px, py] = rec.position, [sx, sy] = rec.size;
    const cx = px + sx / 2, cy = py + sy / 2;
    const rad = ((rec.rotation || 0) * Math.PI) / 180;
    const cos = Math.cos(rad), sin = Math.sin(rad);
    const hx = sx / 2 + margin, hy = sy / 2 + margin;
    const corners = [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]].map(([u, v]) => [cx + u * cos - v * sin, cy + u * sin + v * cos]);
    fillPolygon(busy, [corners], 1);
  };
  for (const rec of roadObjects) mark(rec, 0.08);
  for (const rec of railObjects) mark(rec, 0.08);
  for (const b of buildingObjects) mark(b.rec, 0.1);
  for (const p of placeObjects) mark(p, 0.05);
  const free = (x, y) => x > 0.2 && y > 0.2 && x < proj.width - 0.2 && y < proj.height - 0.2 && sample(busy, x, y, 1) === 0 && !wetAt(x, y);
  const wetAt = (x, y) => {
    const c = Math.round(x * d), r = Math.round(y * d);
    return c < 0 || r < 0 || c >= cols || r >= rows || wetness[r * cols + c] > 0.15;
  };

  // Деревья: в лесу и парках сеткой со сдвигом, во дворах частных домов — по одному-два.
  const treeObjects = [];
  const tree = (x, y, scale) => {
    const style = config.trees;
    const crown = meters(style.crown) * scale;
    treeObjects.push({
      position: [round(x - crown / 2), round(y - crown / 2)],
      size: [round(crown), round(crown)],
      shape: "sphere",
      height: round(meters(style.height) * scale),
      color: style.colors[Math.floor(rand() * style.colors.length)],
      map: true,
    });
    fillPolygon(busy, [[[x - crown / 3, y - crown / 3], [x + crown / 3, y - crown / 3], [x + crown / 3, y + crown / 3], [x - crown / 3, y + crown / 3]]], 1);
  };
  for (const z of zones) {
    const spacing = config.trees.spacing[z.kind];
    if (!spacing) continue;
    const step = meters(spacing);
    for (let y = Math.max(0, z.box[1]); y <= Math.min(proj.height, z.box[3]); y += step) {
      for (let x = Math.max(0, z.box[0]); x <= Math.min(proj.width, z.box[2]); x += step) {
        const jx = x + (rand() - 0.5) * step * 0.8, jy = y + (rand() - 0.5) * step * 0.8;
        if (!inside([jx, jy], z.ring) || !free(jx, jy)) continue;
        tree(jx, jy, 0.75 + rand() * 0.5);
      }
    }
  }
  for (const box of houseSpots) {
    if (rand() > config.trees.yard_chance) continue;
    for (let attempt = 0; attempt < 6; attempt++) {
      const a = rand() * Math.PI * 2, r = box.length / 2 + meters(4 + rand() * 6);
      const x = box.cx + Math.cos(a) * r, y = box.cy + Math.sin(a) * r;
      if (free(x, y)) { tree(x, y, 0.6 + rand() * 0.45); break; }
    }
  }

  // Адреса жилых домов для заказов жителей: настоящие улица и номер, по одному дому на квадрат сетки,
  // точка остановки — на ближайшей проезжей улице.
  const drivableIndex = segmentIndex(drivable, 4);
  const bins = new Map();
  for (const b of buildingObjects) {
    if (!b.address || (b.kind !== "house" && b.kind !== "apartments") || b.rec.place) continue;
    const key = Math.floor(b.box.cx / config.addresses.bin) + "," + Math.floor(b.box.cy / config.addresses.bin);
    if (!bins.has(key)) bins.set(key, []);
    bins.get(key).push(b);
  }
  const addresses = [];
  for (const list of bins.values()) {
    const b = list[Math.floor(rand() * list.length)];
    const stop = drivableIndex.nearest([b.box.cx, b.box.cy], 3);
    if (!stop || stop.distance > config.addresses.max_gap || !main.has(stop.segment)) continue;
    if (stop.point[0] < 1 || stop.point[1] < 1 || stop.point[0] > proj.width - 1 || stop.point[1] > proj.height - 1) continue;
    addresses.push({ address: b.address, kind: b.kind, x: round(stop.point[0]), y: round(stop.point[1]) });
  }
  addresses.sort((p, q) => p.address.localeCompare(q.address, "ru"));

  // Модели вместо коробок и шаров: дома по видам, пятиэтажки секциями, деревья, места игры.
  let buildingRecords = buildingObjects.map((b) => b.rec);
  let treeRecords = treeObjects;
  let placeRecords = placeObjects;
  if (config.models && config.use_models) {
    const index = segmentIndex(roadSegments);
    buildingRecords = buildingObjects.flatMap((b) => dressBuilding(b, config, index, rand));
    buildingRecords = buildingRecords.filter((r) => inScene([r.position[0] + r.size[0] / 2, r.position[1] + r.size[1] / 2]));
    placeRecords = placeObjects.map((rec) => dressPlace(rec, config, index));
    treeRecords = treeObjects.map((rec) => dressTree(rec, config, rand));
  }

  const objects = [
    ...railObjects,
    ...roadObjects,
    ...placeRecords,
    ...deckObjects,
    ...buildingRecords,
    ...treeRecords,
  ];
  return {
    width: proj.width,
    height: proj.height,
    objects,
    heights,
    addresses,
    stats: {
      roads: roadObjects.length,
      bridges: deckObjects.length,
      rail: railObjects.length,
      buildings: buildingObjects.length,
      trees: treeObjects.length,
      places: config.places.length,
      addresses: addresses.length,
      waterPoints: wetness.reduce((s, v) => s + (v > 0.5 ? 1 : 0), 0),
    },
    zones,
    water,
    proj,
  };
}

/** Сетка кусков улиц по клеткам сцены: ближайшая улица к дому ищется рядом, а не по всему городу. */
function segmentIndex(segments, cellSize = 4) {
  const grid = new Map();
  const key = (cx, cy) => cx + "," + cy;
  for (const s of segments) {
    const x0 = Math.floor(Math.min(s.a[0], s.b[0]) / cellSize), x1 = Math.floor(Math.max(s.a[0], s.b[0]) / cellSize);
    const y0 = Math.floor(Math.min(s.a[1], s.b[1]) / cellSize), y1 = Math.floor(Math.max(s.a[1], s.b[1]) / cellSize);
    for (let cx = x0; cx <= x1; cx++) for (let cy = y0; cy <= y1; cy++) {
      const k = key(cx, cy);
      if (!grid.has(k)) grid.set(k, []);
      grid.get(k).push(s);
    }
  }
  return {
    nearest(p, reach = 2) {
      const cx = Math.floor(p[0] / cellSize), cy = Math.floor(p[1] / cellSize);
      let best = null;
      for (let dx = -reach; dx <= reach; dx++) for (let dy = -reach; dy <= reach; dy++) {
        for (const s of grid.get(key(cx + dx, cy + dy)) || []) {
          const n = nearestOnSegment(p, s.a, s.b);
          if (!best || n.distance < best.distance) best = { ...n, segment: s };
        }
      }
      return best;
    },
    near(p) {
      const cx = Math.floor(p[0] / cellSize), cy = Math.floor(p[1] / cellSize);
      const out = new Set();
      for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) for (const s of grid.get(key(cx + dx, cy + dy)) || []) out.add(s);
      return out;
    },
  };
}

/** Куски самой большой связной части сети улиц: туда, куда машина доезжает от конторы. */
function mainNetwork(segments, index) {
  const id = new Map(segments.map((s, i) => [s, i]));
  const parent = segments.map((_, i) => i);
  const find = (i) => (parent[i] === i ? i : (parent[i] = find(parent[i])));
  for (const s of segments) {
    for (const end of [s.a, s.b]) {
      for (const other of index.near(end)) {
        if (other === s) continue;
        const n = nearestOnSegment(end, other.a, other.b);
        if (n.distance <= Math.max(s.width, other.width) / 2 + 0.05) parent[find(id.get(s))] = find(id.get(other));
      }
    }
  }
  const sizes = new Map();
  for (let i = 0; i < segments.length; i++) sizes.set(find(i), (sizes.get(find(i)) || 0) + 1);
  let biggest = null;
  for (const [root, n] of sizes) if (biggest === null || n > sizes.get(biggest)) biggest = root;
  return new Set(segments.filter((_, i) => find(i) === biggest));
}

/** Угол модели: длинная сторона модели — вдоль длинной стороны дома, лицо — к ближайшей улице. */
function facingRotation(model, box, index) {
  const theta = box.angle;
  const candidates = model.long === "y" ? [theta - 90, theta + 90] : [theta, theta + 180];
  const road = index.nearest([box.cx, box.cy]);
  if (!road) return norm360(candidates[0]);
  const to = [road.point[0] - box.cx, road.point[1] - box.cy];
  const score = (r) => {
    const rad = (r * Math.PI) / 180;
    return -Math.sin(rad) * to[0] + Math.cos(rad) * to[1];
  };
  return norm360(score(candidates[0]) >= score(candidates[1]) ? candidates[0] : candidates[1]);
}

function norm360(deg) {
  return round(((deg % 360) + 360) % 360, 2);
}

/** Запись модели на месте дома: тот же прямоугольник, модель вписана в него целиком, без растяжения. */
function modelRecord(rec, model, cx, cy, length, width, scaleRange, config, rotation) {
  const [ml, mw] = model.long === "y" ? [model.size[1], model.size[0]] : model.size;
  const lengthM = length * config.cell, widthM = width * config.cell;
  const fit = Math.min(lengthM / ml, widthM / mw);
  const s = Math.max(scaleRange[0], Math.min(scaleRange[1], fit));
  const { shape: _s, color: _c, height: _h, ...rest } = rec;
  // Модель, длинная вдоль Y, повёрнута на четверть оборота относительно дома: её место на земле —
  // тот же прямоугольник, но с длиной и шириной, отсчитанными от её собственного поворота.
  const [sizeX, sizeY] = model.long === "y" ? [width, length] : [length, width];
  return {
    ...rest,
    ...rectRecord(cx, cy, sizeX, sizeY, rotation),
    model: model.name,
    height: round((model.height * s) / config.cell),
  };
}

function pick(list, rand) {
  return list[Math.floor(rand() * list.length)];
}

function dressBuilding(b, config, index, rand) {
  const cat = config.models;
  const { box, kind, rec } = b;
  const lengthM = box.length * config.cell, widthM = box.width * config.cell;
  const ratio = box.width > 0 ? box.length / box.width : 1;
  if (rec.place) return [dressPlace(rec, config, index)];
  if (kind === "house" && lengthM <= cat.house_max_length) {
    const choices = cat.house.filter((m) => (ratio < 1.15 ? m.square : !m.square || ratio < 1.3));
    const model = pick(choices.length ? choices : cat.house, rand);
    return [modelRecord(rec, model, box.cx, box.cy, box.length, box.width, cat.house_scale, config, facingRotation(model, box, index))];
  }
  if (kind === "apartments" && ratio >= 1.8 && widthM >= 9 && widthM <= 16 && (rec.height * config.cell) <= 17) {
    const model = pick(cat.section, rand);
    const n = Math.max(1, Math.round(lengthM / model.size[0]));
    const part = box.length / n;
    const rotation = facingRotation(model, box, index);
    const rad = (box.angle * Math.PI) / 180;
    const out = [];
    for (let i = 0; i < n; i++) {
      const t = (i + 0.5) * part - box.length / 2;
      out.push(modelRecord(rec, model, box.cx + Math.cos(rad) * t, box.cy + Math.sin(rad) * t, part, box.width, cat.section_scale, config, rotation));
    }
    return out;
  }
  if ((kind === "public" || kind === "shop") && lengthM >= 9 && lengthM <= 26 && ratio < 2.2) {
    const model = pick(cat.old, rand);
    return [modelRecord(rec, model, box.cx, box.cy, box.length, box.width, cat.old_scale, config, facingRotation(model, box, index))];
  }
  return [rec];
}

function dressPlace(rec, config, index) {
  const model = rec.place && config.models.places[rec.place];
  if (!model) return rec;
  const [px, py] = rec.position, [sx, sy] = rec.size;
  const box = { cx: px + sx / 2, cy: py + sy / 2, length: sx, width: sy, angle: rec.rotation || 0 };
  return modelRecord(rec, model, box.cx, box.cy, sx, sy, [0.3, 3], config, facingRotation(model, box, index));
}

function dressTree(rec, config, rand) {
  const model = pick(config.models.trees, rand);
  const s = 0.75 + rand() * 0.5;
  const crown = (model.size[0] * s) / config.cell;
  const cx = rec.position[0] + rec.size[0] / 2, cy = rec.position[1] + rec.size[1] / 2;
  const { shape: _s, color: _c, height: _h, ...rest } = rec;
  return {
    ...rest,
    position: [round(cx - crown / 2), round(cy - crown / 2)],
    size: [round(crown), round(crown)],
    model: model.name,
    height: round((model.height * s) / config.cell),
    rotation: round(rand() * 360, 1),
  };
}

/** Прямые куски ломаной: точки ближе `minLen` и изломы меньше 5° сливаются. */
export function straightPieces(points, minLen) {
  const pts = [];
  for (const p of points) {
    const last = pts[pts.length - 1];
    if (!last || Math.hypot(p[0] - last[0], p[1] - last[1]) >= minLen) pts.push(p);
  }
  if (pts.length < 2) return [];
  const keep = [pts[0]];
  for (let i = 1; i < pts.length - 1; i++) {
    const a = keep[keep.length - 1], b = pts[i], c = pts[i + 1];
    const d1 = Math.atan2(b[1] - a[1], b[0] - a[0]);
    const d2 = Math.atan2(c[1] - b[1], c[0] - b[0]);
    let turn = Math.abs(d2 - d1) * (180 / Math.PI);
    if (turn > 180) turn = 360 - turn;
    if (turn >= COLLINEAR_DEGREES) keep.push(b);
  }
  keep.push(pts[pts.length - 1]);
  const pieces = [];
  for (let i = 0; i + 1 < keep.length; i++) pieces.push([keep[i], keep[i + 1]]);
  return pieces;
}

function isClosed(g) {
  if (!g || g.length < 4) return false;
  const a = g[0], b = g[g.length - 1];
  return a && b && a.lat === b.lat && a.lon === b.lon;
}

function isWaterTags(t) {
  return !!t && (t.natural === "water" || t.waterway === "riverbank");
}

function boundsOf(ring) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of ring) { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
  return [x0, y0, x1, y1];
}

/** Вид земли многоугольника для цвета земли, деревьев и домов. */
export function zoneKind(t) {
  if (t.natural === "wood" || t.landuse === "forest") return "wood";
  if (t.natural === "scrub") return "scrub";
  if (t.leisure === "park" || t.leisure === "garden") return "park";
  if (t.landuse === "grass" || t.natural === "grassland" || t.landuse === "meadow" || t.landuse === "village_green") return "grass";
  if (t.natural === "beach" || t.natural === "sand") return "sand";
  if (t.leisure === "pitch" || t.leisure === "stadium") return "pitch";
  if (["residential", "industrial", "garages", "allotments", "farmland", "cemetery", "commercial", "retail", "construction", "railway", "farmyard"].includes(t.landuse)) return t.landuse;
  return null;
}

/** Вид дома по меткам OpenStreetMap, площади и земле, на которой он стоит. */
export function buildingKind(t, m2, box, zone) {
  const b = t.building;
  if (["house", "detached", "semidetached_house", "bungalow", "cabin", "farm"].includes(b)) return "house";
  if (["apartments", "dormitory"].includes(b)) return "apartments";
  if (["industrial", "warehouse", "factory", "manufacture", "hangar", "service", "storage_tank"].includes(b)) return "industrial";
  if (["garage", "garages", "carport"].includes(b)) return "garage";
  if (["retail", "commercial", "supermarket", "kiosk", "shop"].includes(b)) return "shop";
  if (["church", "chapel", "cathedral", "temple"].includes(b)) return "church";
  if (["school", "kindergarten", "hospital", "public", "civic", "office", "government", "college", "train_station", "transportation", "university"].includes(b)) return "public";
  if (["shed", "hut", "greenhouse", "roof", "toilets"].includes(b)) return "shed";
  if (b === "residential") return m2 > 250 ? "apartments" : "house";
  if (m2 < 30) return "shed";
  if (zone === "garages") return "garage";
  if (zone === "industrial" || zone === "railway") return m2 < 60 ? "shed" : "industrial";
  if (zone === "allotments") return "shed";
  const elongation = box.width > 0 ? box.length / box.width : 1;
  if (m2 <= 220 && elongation < 3) return "house";
  if (m2 > 220 && elongation >= 2.2) return "apartments";
  return m2 > 220 ? "public" : "house";
}

/** Здание места игры: по элементу OpenStreetMap (`w…` или `n…`) или по точке; не здание — самое большое здание внутри. */
function findPlace(place, els, buildings, proj, zones, ringOf) {
  if (place.osm) {
    const type = place.osm[0] === "w" ? "way" : place.osm[0] === "n" ? "node" : "relation";
    const id = Number(place.osm.slice(1));
    const el = els.find((e) => e.type === type && e.id === id);
    if (!el) return null;
    if (type === "way" && el.tags && el.tags.building) {
      const b = buildings.find((x) => x.osm === place.osm);
      return b ? { building: b, center: [b.box.cx, b.box.cy] } : null;
    }
    if (type === "way") {
      const ring = ringOf(el.geometry);
      const inner = buildings.filter((b) => inside([b.box.cx, b.box.cy], ring));
      if (inner.length && !place.site) {
        const big = inner.reduce((m, b) => (b.box.length * b.box.width > m.box.length * m.box.width ? b : m));
        return { building: big, center: [big.box.cx, big.box.cy] };
      }
      const box = orientedBox(ring);
      return { box, center: [box.cx, box.cy] };
    }
    return nearestBuilding(proj.toCell(el.lat, el.lon), buildings);
  }
  return nearestBuilding(proj.toCell(place.lat, place.lon), buildings);
}

function nearestBuilding(p, buildings) {
  let best = null;
  for (const b of buildings) {
    if (b.kind === "shed" || b.kind === "garage") continue;
    const hit = inside(p, b.ring) ? 0 : Math.hypot(b.box.cx - p[0], b.box.cy - p[1]);
    if (!best || hit < best.d) best = { b, d: hit };
  }
  return best ? { building: best.b, center: [best.b.box.cx, best.b.box.cy] } : null;
}

/** «улица Сеченова» → «улице Сеченова»: имя улицы в форме для «на …». */
export function streetForm(name) {
  if (!name) return "";
  return name
    .replace(/^улица /i, "улице ")
    .replace(/ улица$/i, " улице")
    .replace(/^проспект /i, "проспекте ")
    .replace(/ проспект$/i, " проспекте")
    .replace(/^переулок /i, "переулке ")
    .replace(/ переулок$/i, " переулке")
    .replace(/^площадь /i, "площади ")
    .replace(/ площадь$/i, " площади")
    .replace(/ бульвар$/i, " бульваре")
    .replace(/ шоссе$/i, " шоссе")
    .replace(/ набережная$/i, " набережной")
    .replace(/(ая) (улице|площади|набережной)$/i, "ой $2")
    .replace(/(ий|ый) (проспекте|переулке|бульваре)$/i, "ом $2");
}
