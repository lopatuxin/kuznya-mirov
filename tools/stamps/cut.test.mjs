import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { cutFromFile } from "./cut.mjs";
import { TILE_SIZE } from "./tiles.mjs";

const CUT = fileURLToPath(new URL("./cut.mjs", import.meta.url));

const LIST = {
  out: "result/stamps",
  stamps: [
    { name: "first", place: "тест", lat: 50, lon: 87, km: [4, 3], points: 24 },
    { name: "second", place: "тест", lat: 50.1, lon: 87.1, km: [3, 4], points: 24 },
  ],
};

/** Плитки с холмом: высота зависит от номера плитки и места в ней, так что разные плитки не совпадают. */
async function hillTile(tileX, tileY) {
  const heights = new Float64Array(TILE_SIZE * TILE_SIZE);
  for (let i = 0; i < heights.length; i++) heights[i] = 500 + 300 * Math.sin((tileX * 31 + (i % TILE_SIZE)) / 45) * Math.cos((tileY * 17 + (i >> 8)) / 60);
  return heights;
}

describe("вырезка по списку", () => {
  let dir;
  let listPath;
  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "stamps-cut-"));
    listPath = join(dir, "list.json");
    writeFileSync(listPath, JSON.stringify(LIST));
  });
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("пишет <out>/<имя>.json для каждого выреза, папка out — от файла списка, те же плитки — те же файлы", async () => {
    const files = await cutFromFile(listPath, hillTile);
    assert.deepEqual(
      files.map((file) => file.replaceAll("\\", "/").slice(dir.length + 1)),
      ["result/stamps/first.json", "result/stamps/second.json"],
    );
    const written = files.map((file) => readFileSync(file, "utf8"));
    const heights = JSON.parse(written[0]).heights;
    assert.deepEqual([heights[0].length, heights.length], [24, 18]);
    assert.ok(written[0].startsWith('{ "heights": [\n  ['));
    await cutFromFile(listPath, hillTile);
    assert.deepEqual(
      files.map((file) => readFileSync(file, "utf8")),
      written,
    );
  });

  it("плитка не скачалась — программа останавливается с номером выреза и плитки, ни одного файла", async () => {
    let calls = 0;
    const failing = async (tileX, tileY) => {
      calls += 1;
      if (calls > 4) throw new Error(`плитка 13/${tileX}/${tileY}: ответ 403`);
      return hillTile(tileX, tileY);
    };
    await assert.rejects(cutFromFile(listPath, failing), /вырез «.+»: плитка 13\/\d+\/\d+: ответ 403/);
    assert.equal(existsSync(join(dir, "result")), false);
  });

  it("ошибка списка называет вырез и поле, файлы не пишутся", async () => {
    const bad = { ...LIST, stamps: [LIST.stamps[0], { ...LIST.stamps[1], km: [0, 4] }] };
    writeFileSync(listPath, JSON.stringify(bad));
    await assert.rejects(cutFromFile(listPath, hillTile), /вырез 2 «second»: «km»/);
    assert.deepEqual(readdirSync(dir), ["list.json"]);
  });
});

describe("cut.mjs", () => {
  let dir;
  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "stamps-cli-"));
  });
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  function run(...args) {
    return spawnSync(process.execPath, [CUT, ...args], { encoding: "utf8" });
  }

  it("без списка, со сломанным JSON и со списком с ошибкой — код 1, сообщение, ничего не записано", () => {
    assert.equal(run().status, 1);
    const broken = join(dir, "broken.json");
    writeFileSync(broken, "{ stamps: ");
    const brokenResult = run(broken);
    assert.equal(brokenResult.status, 1);
    assert.ok(brokenResult.stderr.includes("broken.json"));
    const duplicate = join(dir, "duplicate.json");
    writeFileSync(duplicate, JSON.stringify({ ...LIST, stamps: [LIST.stamps[0], LIST.stamps[0]] }));
    const duplicateResult = run(duplicate);
    assert.equal(duplicateResult.status, 1);
    assert.match(duplicateResult.stderr, /вырез 2 «first»: это имя уже есть/);
    assert.deepEqual(readdirSync(dir).sort(), ["broken.json", "duplicate.json"]);
  });
});
