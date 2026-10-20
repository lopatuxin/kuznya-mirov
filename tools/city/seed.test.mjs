import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { buildCity } from "./city.mjs";
import { projector } from "./geo.mjs";
import { planTown, readSeedConfig } from "./seed.mjs";

const config = await readSeedConfig(fileURLToPath(new URL("../../art/transport/seed.json", import.meta.url)));
const clients = JSON.parse(readFileSync(new URL("../../games/transport/tables/clients.json", import.meta.url), "utf8"));

const cityOf = (cfg) => {
  const plan = planTown(cfg);
  return { plan, city: buildCity({ elements: plan.elements }, { ...cfg, places: plan.places }) };
};

test("зародыш: одни и те же настройки — та же карта", () => {
  assert.deepEqual(planTown(config), planTown(config));
});

test("зародыш: все места, откуда и куда возят клиенты, на карте и с остановкой", () => {
  const { city } = cityOf(config);
  const places = new Map(city.objects.filter((o) => o.place).map((o) => [o.place, o]));
  const needed = new Set(["office"]);
  for (const client of Object.values(clients)) for (const job of client.jobs) for (const end of [job.from, job.to]) needed.add(end);
  for (const end of needed) {
    if (end === "address") assert.ok(city.addresses.length >= 6, `адресов ${city.addresses.length}`);
    else if (end === "shops") assert.ok([...places.keys()].some((id) => id.startsWith("shop_")), "нет магазинов");
    else assert.ok(places.has(end), `нет места «${end}»`);
  }
  for (const p of places.values()) assert.ok(Number.isFinite(p.stop_x) && Number.isFinite(p.stop_y), `у «${p.place}» нет остановки`);
  // Над местом — значок его дела вместо подписи: у каждого места значок, он объявлен картинкой игры и файл есть.
  const images = JSON.parse(readFileSync(new URL("../../games/transport/game.json", import.meta.url), "utf8")).files.images ?? {};
  for (const p of places.values()) {
    assert.ok(images[p.icon], `значок «${p.icon}» места «${p.place}» не объявлен в files.images`);
    assert.ok(existsSync(new URL(`../../games/transport/${images[p.icon].path}`, import.meta.url)), `нет файла значка «${p.icon}»`);
  }
});

test("зародыш: дома 1930-х — избы, деревянные и каменные двухэтажные, без пятиэтажек", () => {
  const { city, plan } = cityOf(config);
  const models = new Set(city.objects.map((o) => o.model).filter(Boolean));
  assert.ok(!models.has("khrush_grey") && !models.has("khrush_sand"), "в зародыше пятиэтажка");
  assert.ok([...models].some((m) => m.startsWith("izba")), "в зародыше нет изб");
  assert.ok(plan.stats.blocks === config.town.blocks.length, `кварталов ${plan.stats.blocks}`);
  assert.ok(config.town.names.includes(plan.name), plan.name);
});

test("зародыш: дома не налезают друг на друга и на улицы", () => {
  const { plan } = cityOf(config);
  const proj = projector(config.bbox, config.cell);
  const cells = (e) => e.geometry.map((g) => proj.toCell(g.lat, g.lon));
  const boxOf = (id, pts, kind) => {
    const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
    return { id, kind, pts, x0: Math.min(...xs), x1: Math.max(...xs), y0: Math.min(...ys), y1: Math.max(...ys) };
  };
  const boxes = plan.elements.filter((e) => e.type === "way" && e.tags.building).map((e) => boxOf(e.id, cells(e).slice(0, -1), "здание"));
  // Улица — прямоугольник своей ширины вдоль каждого куска.
  for (const e of plan.elements.filter((w) => w.type === "way" && w.tags.highway)) {
    const half = config.roads[e.tags.highway].width / config.cell / 2;
    const pts = cells(e);
    for (let i = 0; i + 1 < pts.length; i++) {
      const [a, b] = [pts[i], pts[i + 1]];
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      if (len < 1e-6) continue;
      const n = [((a[1] - b[1]) / len) * half, ((b[0] - a[0]) / len) * half];
      boxes.push(boxOf(e.id, [[a[0] + n[0], a[1] + n[1]], [b[0] + n[0], b[1] + n[1]], [b[0] - n[0], b[1] - n[1]], [a[0] - n[0], a[1] - n[1]]], "улица"));
    }
  }
  // Пересечение выпуклых четырёхугольников: ни одна сторона одного не отделяет другой.
  const separated = (a, b) => {
    for (const poly of [a, b]) {
      for (let i = 0; i < poly.length; i++) {
        const p = poly[i], q = poly[(i + 1) % poly.length];
        const n = [q[1] - p[1], p[0] - q[0]];
        const proj = (pts) => pts.map((r) => r[0] * n[0] + r[1] * n[1]);
        const pa = proj(a), pb = proj(b);
        if (Math.max(...pa) <= Math.min(...pb) + 1e-12 || Math.max(...pb) <= Math.min(...pa) + 1e-12) return true;
      }
    }
    return false;
  };
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i], b = boxes[j];
      if (a.kind === "улица" && b.kind === "улица") continue;
      if (a.x1 < b.x0 || b.x1 < a.x0 || a.y1 < b.y0 || b.y1 < a.y0) continue;
      assert.ok(separated(a.pts, b.pts), `${a.kind} ${a.id} и ${b.kind} ${b.id} налезают друг на друга`);
    }
  }
});

test("зародыш: любой номер случайности даёт другой городок, и он строится целиком", () => {
  // Перевод карты сам бросает ошибку, если место не нашлось или его остановка не на общей сети улиц; что
  // остановка у самого места, а не на дальней улице, проверяется здесь.
  const proj = projector(config.bbox, config.cell);
  const cells = (e) => e.geometry.map((g) => proj.toCell(g.lat, g.lon));
  const first = planTown(config).elements[0].geometry;
  for (let seed = 1; seed <= 60; seed++) {
    const { plan, city } = cityOf({ ...config, town: { ...config.town, seed } });
    if (seed !== config.town.seed) assert.notDeepEqual(plan.elements[0].geometry, first, `номер ${seed}`);
    assert.ok(city.addresses.length >= 6, `номер ${seed}: адресов ${city.addresses.length}`);
    for (const p of city.objects.filter((o) => o.place)) {
      const far = Math.hypot(p.position[0] + p.size[0] / 2 - p.stop_x, p.position[1] + p.size[1] / 2 - p.stop_y) * config.cell;
      assert.ok(far <= 100, `номер ${seed}: остановка «${p.place}» в ${Math.round(far)} м от него`);
    }
    // Городок — только кварталы: улицы 1930-х грунтовые, и ни улица, ни здание не выходят за кварталы дальше
    // половины ширины главной улицы.
    const town = plan.elements.filter((e) => e.tags.landuse === "residential" || e.tags.landuse === "retail").flatMap(cells);
    assert.equal(town.length / 5, config.town.blocks.length + 1, `номер ${seed}: кварталов на земле`);
    const reach = config.roads.secondary.width / config.cell / 2 + 1e-6;
    const [x0, x1] = [Math.min(...town.map((p) => p[0])) - reach, Math.max(...town.map((p) => p[0])) + reach];
    const [y0, y1] = [Math.min(...town.map((p) => p[1])) - reach, Math.max(...town.map((p) => p[1])) + reach];
    for (const e of plan.elements.filter((w) => w.type === "way" && (w.tags.highway || w.tags.building))) {
      if (e.tags.highway) assert.equal(e.tags.surface, "unpaved", `номер ${seed}: ${e.tags.name ?? e.tags.highway} не грунтовая`);
      for (const [x, y] of cells(e)) {
        assert.ok(x >= x0 && x <= x1 && y >= y0 && y <= y1, `номер ${seed}: ${e.tags.name ?? e.tags.building} за кварталами`);
      }
    }
    assert.ok(!plan.elements.some((e) => e.tags.railway), `номер ${seed}: железная дорога`);
  }
});
