import assert from "node:assert/strict";
import { test } from "node:test";
import { inflateSync } from "node:zlib";
import { buildingKind, buildCity, straightPieces, streetForm, zoneKind } from "./city.mjs";
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
