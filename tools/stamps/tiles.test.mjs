import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { TILE_SIZE, createTileReader, terrariumHeight, tileUrl } from "./tiles.mjs";
import { encodePng } from "./testPng.mjs";

/** Плитка, где точка `(x, y)` — высота `metersAt(x, y)` в кодировке Terrarium, RGB или RGBA. */
function tilePng(metersAt, channels = 3, size = TILE_SIZE) {
  const pixels = new Uint8Array(size * size * channels);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const value = Math.round((metersAt(x, y) + 32768) * 256);
      pixels.set([value >> 16, (value >> 8) & 255, value & 255], (y * size + x) * channels);
      if (channels === 4) pixels[(y * size + x) * channels + 3] = 255;
    }
  }
  return encodePng({ width: size, height: size, channels, pixels, filters: [4, 1, 2] });
}

describe("terrariumHeight", () => {
  it("высота в метрах — красный × 256 плюс зелёный плюс синий / 256 минус 32768", () => {
    assert.equal(terrariumHeight(128, 0, 0), 0);
    assert.equal(terrariumHeight(128, 1, 128), 1.5);
    assert.equal(terrariumHeight(131, 200, 64), 3 * 256 + 200 + 0.25);
    assert.equal(terrariumHeight(0, 0, 0), -32768);
    assert.equal(terrariumHeight(127, 255, 255), -1 / 256);
  });

  it("адрес плитки — масштаб 13, колонка и строка", () => {
    assert.equal(tileUrl(6120, 2650), "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/13/6120/2650.png");
  });
});

describe("createTileReader", () => {
  let cacheDir;
  beforeEach(() => {
    cacheDir = mkdtempSync(join(tmpdir(), "stamps-cache-"));
  });
  afterEach(() => rmSync(cacheDir, { recursive: true, force: true }));

  it("плитка из PNG — высоты в метрах строками сверху вниз, RGB и RGBA", async () => {
    for (const channels of [3, 4]) {
      const read = createTileReader({ cacheDir: join(cacheDir, String(channels)), download: async () => tilePng((x, y) => x * 2 + y * 0.5 - 100, channels) });
      const heights = await read(1, 2);
      assert.equal(heights.length, TILE_SIZE * TILE_SIZE);
      assert.equal(heights[0], -100);
      assert.equal(heights[5 * TILE_SIZE + 3], 6 + 2.5 - 100);
    }
  });

  it("скачанная плитка ложится в кэш и второй раз не качается, даже новым читателем", async () => {
    const asked = [];
    const download = async (url) => {
      asked.push(url);
      return tilePng(() => 7);
    };
    const first = await createTileReader({ cacheDir, download })(10, 20);
    await createTileReader({ cacheDir, download })(10, 20);
    const reader = createTileReader({ cacheDir, download });
    assert.deepEqual(await Promise.all([reader(10, 20), reader(10, 20)]), [first, first]);
    assert.deepEqual(asked, [tileUrl(10, 20)]);
    assert.deepEqual(readdirSync(cacheDir), ["13-10-20.png"]);
  });

  it("плитка не скачалась — ошибка с номером и адресом плитки, кэш пуст", async () => {
    const read = createTileReader({
      cacheDir,
      download: async () => {
        throw new Error("ответ 403 Forbidden");
      },
    });
    await assert.rejects(read(5, 6), (error) => error.message.includes("13/5/6") && error.message.includes(tileUrl(5, 6)) && error.message.includes("403"));
    assert.equal(existsSync(join(cacheDir, "13-5-6.png")), false);
  });

  it("ответ, что не PNG, и плитка не того размера — ошибка, в кэш не попадают", async () => {
    await assert.rejects(createTileReader({ cacheDir, download: async () => Buffer.from("<html>") })(1, 1), /13\/1\/1.*нет подписи/);
    await assert.rejects(createTileReader({ cacheDir, download: async () => tilePng(() => 0, 3, 4) })(1, 2), /размер плитки 4 × 4/);
    assert.deepEqual(readdirSync(cacheDir), []);
  });
});
