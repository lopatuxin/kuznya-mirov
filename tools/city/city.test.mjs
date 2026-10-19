import assert from "node:assert/strict";
import { test } from "node:test";
import { inflateSync } from "node:zlib";
import { buildingKind, buildCity, signSpeed, straightPieces, streetForm, zoneKind } from "./city.mjs";
import { area, clipSegment, inside, orientedBox, projector, rectRecord } from "./geo.mjs";
import { encodePng } from "./png.mjs";
import { createRaster, drawLine, fillPolygon, floodFill, sample } from "./raster.mjs";

test("перевод в клетки: север вверх, восток вправо, размер по метрам", () => {
  const p = projector([57.0, 42.0, 57.01, 42.02], 20);
  const [x0, y0] = p.toCell(57.01, 42.0);
  assert.deepEqual([Math.round(x0), Math.round(y0)], [0, 0]);
  const [x1, y1] = p.toCell(57.0, 42.02);
  assert.equal(Math.round(y1), Math.round(0.01 * 111320 / 20));
  assert.ok(x1 > 60 && x1 < 70, `ширина ${x1}`);
  assert.equal(p.width, Math.round(x1));
});

test("наименьший прямоугольник повёрнутого дома: длина, ширина, угол", () => {
  const rad = (30 * Math.PI) / 180;
  const pts = [[-4, -2], [4, -2], [4, 2], [-4, 2]].map(([x, y]) => [10 + x * Math.cos(rad) - y * Math.sin(rad), 5 + x * Math.sin(rad) + y * Math.cos(rad)]);
  const box = orientedBox(pts);
  assert.ok(Math.abs(box.length - 8) < 1e-6);
  assert.ok(Math.abs(box.width - 4) < 1e-6);
  assert.ok(Math.abs(box.angle - 30) < 1e-6);
  assert.ok(Math.abs(box.cx - 10) < 1e-6 && Math.abs(box.cy - 5) < 1e-6);
});

test("запись прямоугольника: position — левый верхний угол до поворота", () => {
  assert.deepEqual(rectRecord(10, 5, 8, 2, 45), { position: [6, 4], size: [8, 2], rotation: 45 });
});

test("площадь и попадание в многоугольник", () => {
  const sq = [[0, 0], [4, 0], [4, 3], [0, 3]];
  assert.equal(area(sq), 12);
  assert.ok(inside([1, 1], sq));
  assert.ok(!inside([5, 1], sq));
});

test("обрезка отрезка рамкой сцены", () => {
  assert.deepEqual(clipSegment([-5, 5], [5, 5], 10, 10), [[0, 5], [5, 5]]);
  assert.equal(clipSegment([-5, -5], [-1, -1], 10, 10), null);
});

test("прямые куски: почти прямая ломаная сливается, излом остаётся", () => {
  const pieces = straightPieces([[0, 0], [5, 0.1], [10, 0], [10, 6]], 0.05);
  assert.equal(pieces.length, 2);
  assert.deepEqual(pieces[0], [[0, 0], [10, 0]]);
});

test("заливка от затравки не перешагивает берег, проведённый наискосок", () => {
  const r = createRaster(10, 10, 4);
  drawLine(r, [[0, 0], [10, 10]], 2);
  const filled = floodFill(r, [8, 2], 0, 1);
  assert.ok(filled > 0);
  assert.equal(sample(r, 2, 8), 0, "по другую сторону берега суша");
  assert.equal(sample(r, 8, 2), 1);
});

test("заливка многоугольника с дырой — остров остаётся сушей", () => {
  const r = createRaster(10, 10, 2);
  fillPolygon(r, [[[1, 1], [9, 1], [9, 9], [1, 9]], [[4, 4], [6, 4], [6, 6], [4, 6]]], 1);
  assert.equal(sample(r, 2, 2), 1);
  assert.equal(sample(r, 5, 5), 0);
});

test("PNG: заголовок и точки разжимаются обратно", () => {
  const png = encodePng(2, 1, Uint8Array.from([255, 0, 0, 255, 0, 0, 255, 128]));
  assert.deepEqual([...png.subarray(1, 4)], [0x50, 0x4e, 0x47]);
  assert.equal(png.readUInt32BE(16), 2);
  assert.equal(png.readUInt32BE(20), 1);
  const idat = png.indexOf("IDAT");
  const len = png.readUInt32BE(idat - 4);
  const raw = inflateSync(png.subarray(idat + 4, idat + 4 + len));
  assert.deepEqual([...raw], [0, 255, 0, 0, 255, 0, 0, 255, 128]);
});

test("виды домов и земли по меткам", () => {
  const box = { length: 2, width: 1 };
  assert.equal(buildingKind({ building: "house" }, 100, box, null), "house");
  assert.equal(buildingKind({ building: "yes" }, 600, box, "industrial"), "industrial");
  assert.equal(buildingKind({ building: "yes" }, 20, box, null), "shed");
  assert.equal(buildingKind({ building: "yes" }, 900, { length: 60, width: 12 }, "residential"), "apartments");
  assert.equal(zoneKind({ natural: "wood" }), "wood");
  assert.equal(zoneKind({ landuse: "garages" }), "garages");
  assert.equal(zoneKind({ amenity: "school" }), null);
});

test("имя улицы для «на …»", () => {
  assert.equal(streetForm("улица Сеченова"), "улице Сеченова");
  assert.equal(streetForm("Комсомольская улица"), "Комсомольской улице");
  assert.equal(streetForm("Волжский бульвар"), "Волжском бульваре");
  assert.equal(streetForm(""), "");
});

test("маленький город: улица с признаком road, дом, вода залита, место у улицы", () => {
  const g = (pts) => pts.map(([lat, lon]) => ({ lat, lon }));
  const osm = {
    elements: [
      { type: "way", id: 1, tags: { highway: "residential", name: "улица Тестовая" }, geometry: g([[57.005, 42.001], [57.005, 42.019]]) },
      { type: "way", id: 2, tags: { building: "house" }, geometry: g([[57.0056, 42.005], [57.0056, 42.0052], [57.0058, 42.0052], [57.0058, 42.005], [57.0056, 42.005]]) },
      { type: "way", id: 3, tags: { natural: "water" }, geometry: g([[57.001, 42.010], [57.001, 42.018], [57.003, 42.018], [57.003, 42.010], [57.001, 42.010]]) },
    ],
  };
  const config = {
    bbox: [57.0, 42.0, 57.01, 42.02], cell: 20, minute_seconds: 3, seed: 1, background: "#808080",
    roads: { residential: { width: 7, color: "#777777", layer: 3, speed: 25 } }, unpaved_color: "#888888", unpaved_speed: 15,
    deck_thickness: 1.5, deck_color: "#999999", rail: { width: 4, color: "#555555" },
    water: { per: 4, seeds: [], depth: -0.3, level: -0.08, color: "#336699" },
    buildings: { house: { height: 6, colors: ["#aa8866"] } },
    trees: { crown: 7, height: 9, colors: ["#336633"], spacing: {}, yard_chance: 0 },
    places: [{ id: "home", name: "Дом на {street}", osm: "w2" }],
  };
  const city = buildCity(osm, config);
  const road = city.objects.find((o) => o.road);
  assert.equal(road.name, "улица Тестовая");
  assert.ok(Math.abs(road.road_speed - (25000 / 60 / 20 / 3)) < 0.01);
  const home = city.objects.find((o) => o.place === "home");
  assert.equal(home.name, "Дом на улице Тестовая");
  assert.ok(Math.abs(home.stop_y - road.position[1] - road.size[1] / 2) < 0.05, "остановка на оси улицы");
  assert.ok(city.stats.waterPoints > 0, "озеро залито");
  assert.ok(city.objects.every((o) => o.map), "все объекты карты помечены");
});

test("предел скорости: знак из maxspeed сильнее вида улицы, российские обозначения — в км/ч", () => {
  assert.equal(signSpeed("40"), 40);
  assert.equal(signSpeed("RU:urban"), 60);
  assert.equal(signSpeed("RU:rural"), 90);
  assert.equal(signSpeed("RU:living_street"), 20);
  assert.equal(signSpeed(undefined), undefined);
  assert.equal(signSpeed("none"), undefined);
  const g = (pts) => pts.map(([lat, lon]) => ({ lat, lon }));
  const osm = {
    elements: [
      { type: "way", id: 1, tags: { highway: "residential", name: "улица Школьная", maxspeed: "40" }, geometry: g([[57.005, 42.001], [57.005, 42.019]]) },
      { type: "way", id: 2, tags: { building: "house" }, geometry: g([[57.0056, 42.005], [57.0056, 42.0052], [57.0058, 42.0052], [57.0058, 42.005], [57.0056, 42.005]]) },
      { type: "way", id: 4, tags: { highway: "residential", name: "улица Грунтовая", maxspeed: "60", surface: "dirt" }, geometry: g([[57.008, 42.001], [57.008, 42.019]]) },
      { type: "way", id: 5, tags: { highway: "living_street", name: "Дворовый проезд" }, geometry: g([[57.002, 42.001], [57.002, 42.019]]) },
      { type: "node", id: 7, lat: 57.0053, lon: 42.0052, tags: { amenity: "fuel" } },
    ],
  };
  const config = {
    bbox: [57.0, 42.0, 57.01, 42.02], cell: 20, minute_seconds: 3, seed: 1, background: "#808080",
    roads: {
      residential: { width: 7, color: "#777777", layer: 3, speed: 60 },
      living_street: { width: 6, color: "#777777", layer: 3, speed: 20 },
    },
    unpaved_color: "#888888", unpaved_speed: 40,
    deck_thickness: 1.5, deck_color: "#999999", rail: { width: 4, color: "#555555" },
    water: { per: 4, seeds: [], depth: -0.3, level: -0.08, color: "#336699" },
    buildings: { house: { height: 6, colors: ["#aa8866"] } },
    trees: { crown: 7, height: 9, colors: ["#336633"], spacing: {}, yard_chance: 0 },
    places: [{ id: "fuel_test", name: "АЗС на {street}", osm: "n7", pad: [30, 20], color: "#8d959c" }],
  };
  const city = buildCity(osm, config);
  const kmh = (name) => (city.objects.find((o) => o.road && o.name === name).road_speed * 60 * 20 * 3) / 1000;
  assert.ok(Math.abs(kmh("улица Школьная") - 40) < 0.01, "знак 40 вместо 60 по виду улицы");
  assert.ok(Math.abs(kmh("улица Грунтовая") - 40) < 0.01, "на грунтовке знак 60 не быстрее покрытия");
  assert.ok(Math.abs(kmh("Дворовый проезд") - 20) < 0.01, "без знака — по виду улицы");
  const fuel = city.objects.find((o) => o.place === "fuel_test");
  assert.equal(fuel.name, "АЗС на улице Школьная");
  assert.ok(Math.abs(fuel.size[0] - 1.5) < 1e-6 && Math.abs(fuel.size[1] - 1) < 1e-6, "площадка 30 × 20 м");
  const houses = city.objects.filter((o) => o.height && !o.road);
  assert.ok(houses.length > 0 && houses.every((h) => h.place === undefined), "точка заправки у дома не делает дом заправкой");
});
