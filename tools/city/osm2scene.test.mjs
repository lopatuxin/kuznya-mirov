import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { buildCity } from "./city.mjs";
import { writeCity } from "./osm2scene.mjs";
import { planTown, readSeedConfig } from "./seed.mjs";

test("запись карты в игру: машина у конторы, камера над ней, название города, виды дорог, модели по карте, видам и коду", async () => {
  const config = await readSeedConfig(fileURLToPath(new URL("../../art/transport/seed.json", import.meta.url)));
  const plan = planTown(config);
  const city = buildCity({ elements: plan.elements }, { ...config, places: plan.places });
  const root = await mkdtemp(join(tmpdir(), "city-"));
  try {
    const out = join(root, "game");
    await mkdir(join(out, "tables"), { recursive: true });
    const game = {
      scene: { player_camera: { start: [0, 0] } },
      files: { rules: "rules.json", code: "code.lua", tables: { t: "tables/t.json" }, models: { khrush_grey: { path: "models/khrush_grey.glb" }, gazel: { path: "models/gazel.glb" } } },
    };
    await writeFile(join(out, "game.json"), (JSON.stringify(game, null, 2) + "\n").replace(/\n/g, "\r\n"));
    await writeFile(join(out, "rules.json"), "{}");
    await writeFile(join(out, "code.lua"), 'create({ model = "gazel_green" })');
    await writeFile(join(out, "tables/t.json"), "[]");
    const author = [{ name: "game", game: true }, { name: "Машина", car: true, size: [0.2, 0.5], model: "gazel", position: [1, 1] }];
    await writeFile(join(out, "scene.json"), JSON.stringify({ objects: author }));
    await writeCity(city, { ...config, out }, "Пробный", root);

    const office = city.objects.find((o) => o.place === "office");
    const scene = JSON.parse(await readFile(join(out, "scene.json"), "utf8"));
    const car = scene.objects.find((o) => o.car);
    assert.deepEqual(car.position.map((v) => Math.round(v * 100)), [office.stop_x - 0.1, office.stop_y - 0.25].map((v) => Math.round(v * 100)));
    assert.equal(scene.objects.find((o) => o.game).town, "Пробный");
    const text = await readFile(join(out, "game.json"), "utf8");
    assert.ok(text.includes("\r\n"), "переводы строк game.json сменились");
    const written = JSON.parse(text);
    assert.deepEqual(written.scene.player_camera.start, [Math.round(office.stop_x * 10) / 10, Math.round(office.stop_y * 10) / 10]);
    const models = Object.keys(written.files.models);
    assert.deepEqual(written.scene.road_kinds, config.road_kinds);
    const ofRoads = Object.values(config.road_kinds).flatMap((k) => [k.piece, k.node].flat());
    const expected = [...new Set([...scene.objects.map((o) => o.model).filter(Boolean), ...ofRoads])].sort();
    assert.ok(scene.objects.some((o) => o.road_kind === "main") && scene.objects.every((o) => !o.road_kind || !o.color), "улицы зародыша — без вида или с цветом");
    assert.ok(!models.includes("khrush_grey"), "модель прежней карты осталась");
    assert.ok(!models.includes("gazel_green"), "модель, которой нет среди объявленных, добавилась из кода");
    assert.deepEqual(models, expected);
    assert.deepEqual(written.files.models.izba_green_red, { path: "models/izba_green_red.glb" });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
