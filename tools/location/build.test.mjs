import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { buildCovers, buildGrid } from "./build.mjs";
import { hundredths, terrainText } from "./files.mjs";
import { decodeGrayPng } from "./pngDecoder.mjs";
import { Grid } from "./terrain.mjs";

const BUILD = fileURLToPath(new URL("./build.mjs", import.meta.url));

const MOUNTAINS = { op: "range", name: "горы", foot: [[0, 0], [9, 0]], side: "left", height: 3, depth: 2 };

function plan(overrides = {}) {
  return {
    size: [16, 12],
    seed: 5,
    terrain: "terrain.json",
    water: { level: -0.5, color: "#3f7fd0" },
    operations: [
      { op: "noise", amplitude: 0.5, wavelength: 5 },
      { op: "range", name: "горы", foot: [[-2, 4], [18, 4]], side: "left", height: 6, depth: 4 },
      { op: "hill", at: [8, 8], radius: 3, height: 1 },
      { op: "pad", area: [[2, 7], [5, 7], [5, 10], [2, 10]], rim: 1 },
      { op: "channel", name: "ручей", line: [[0, 11], [16, 9]], width: 2, bottom: -1, bank: 1 },
      { op: "smooth", passes: 1 },
    ],
    ...overrides,
  };
}

function run(dir, description) {
  const file = join(dir, "plan.json");
  writeFileSync(file, typeof description === "string" ? description : JSON.stringify(description));
  return spawnSync(process.execPath, [BUILD, file], { encoding: "utf8" });
}

describe("terrainText", () => {
  it("вода первой строкой, строка сетки на строку, сотые без лишних нулей, -0 — 0", () => {
    const grid = new Grid(1, 1);
    grid.h.set([0, 1.5, -2.346, -0.001, 3.1, 0.004, 7, 8.2, -9]);
    assert.equal(
      terrainText(grid, { level: -0.5, color: "#3f7fd0" }),
      [
        "{",
        '  "water": { "level": -0.5, "color": "#3f7fd0" },',
        '  "heights": [',
        "    [0, 1.5, -2.35],",
        "    [0, 3.1, 0],",
        "    [7, 8.2, -9]",
        "  ]",
        "}",
        "",
      ].join("\n"),
    );
  });

  it("без воды — без строки water", () => {
    assert.ok(!terrainText(new Grid(1, 1)).includes("water"));
    assert.equal(hundredths(-0.004), "0");
  });

  it("вид файла: water, covers по слою на строку, stamps по горе на строку, затем heights", () => {
    const stamps = [
      { stamp: "beluha", position: [8, 0], size: [38, 32], height: 15 },
      { stamp: "chuya", position: [104.004, 4], size: [40, 30.126], height: 18, rotation: 345 },
      { stamp: "flat", position: [1, 2], size: [3, 4], height: 5, rotation: 0 },
    ];
    const covers = [{ material: "grass" }, { material: "rock", slope: 34 }, { material: "moss", slope: 20.5, mask: "terrain/moss.png" }];
    assert.equal(
      terrainText(new Grid(1, 1, 1), { level: -2.2, color: "#3f7fd0" }, covers, stamps),
      [
        "{",
        '  "water": { "level": -2.2, "color": "#3f7fd0" },',
        '  "covers": [',
        '    { "material": "grass" },',
        '    { "material": "rock", "slope": 34 },',
        '    { "material": "moss", "slope": 20.5, "mask": "terrain/moss.png" }',
        "  ],",
        '  "stamps": [',
        '    { "stamp": "beluha", "position": [8, 0], "size": [38, 32], "height": 15 },',
        '    { "stamp": "chuya", "position": [104, 4], "size": [40, 30.13], "height": 18, "rotation": 345 },',
        '    { "stamp": "flat", "position": [1, 2], "size": [3, 4], "height": 5 }',
        "  ],",
        '  "heights": [',
        "    [0, 0],",
        "    [0, 0]",
        "  ]",
        "}",
        "",
      ].join("\n"),
    );
  });

  it("пустой список гор и его отсутствие — без раздела stamps", () => {
    assert.ok(!terrainText(new Grid(1, 1), undefined, undefined, []).includes("stamps"));
    assert.ok(!terrainText(new Grid(1, 1)).includes("stamps"));
  });
});

describe("buildGrid", () => {
  it("одно описание — один и тот же файл байт в байт, другое зерно — другие неровности", () => {
    const text = (description) => terrainText(buildGrid(description), description.water);
    assert.equal(text(plan()), text(plan()));
    assert.notEqual(text(plan()), text(plan({ seed: 6 })));
  });

  it("неровности операции зависят от её имени, а не от места: вставка операции не меняет соседей", () => {
    const waves = { op: "noise", name: "волны", amplitude: 1, wavelength: 5 };
    const far = { op: "hill", name: "далёкий холм", at: [500, 500], radius: 2, height: 1 };
    const alone = buildGrid(plan({ operations: [waves] }));
    const after = buildGrid(plan({ operations: [far, waves] }));
    assert.deepEqual(after.h, alone.h);
  });

  it("размер сетки — по размеру сцены", () => {
    const grid = buildGrid(plan());
    assert.equal(grid.rows, 25);
    assert.equal(grid.cols, 33);
  });
});

describe("ошибки описания", () => {
  const hill = { op: "hill", at: [1, 1], radius: 2, height: 1 };
  const cases = [
    [plan({ operations: [{ op: "mountain" }] }), "операция 1: неизвестная операция «mountain»"],
    [plan({ operations: [MOUNTAINS, { op: "strata", range: "горы", step: 1 }] }), "операция 2: неизвестная операция «strata»"],
    [plan({ operations: [MOUNTAINS, { op: "blocks", range: "горы", size: 2, amount: 1 }] }), "операция 2: неизвестная операция «blocks»"],
    [plan({ operations: [MOUNTAINS, { op: "cracks", range: "горы", size: 2, width: 1, depth: 1 }] }), "операция 2: неизвестная операция «cracks»"],
    [plan({ operations: [MOUNTAINS, { op: "erosion", range: "горы" }] }), "операция 2: неизвестная операция «erosion»"],
    [plan({ operations: [MOUNTAINS, { op: "talus", range: "горы" }] }), "операция 2: неизвестная операция «talus»"],
    [plan({ operations: [{ ...MOUNTAINS, profile: "middle" }] }), "операция 1 «горы»: неизвестный ключ «profile»"],
    [plan({ operations: [{ ...MOUNTAINS, spurs: 3 }] }), "операция 1 «горы»: неизвестный ключ «spurs»"],
    [plan({ tint: { range: "горы" } }), "неизвестный ключ описания «tint»"],
    [plan({ operations: [{ op: "constructor" }] }), "операция 1: неизвестная операция «constructor»"],
    [plan({ operations: [{ op: "hill", name: "бугор", at: [1, 1], radius: 2 }] }), "операция 1 «бугор»: нет обязательного ключа «height»"],
    [plan({ operations: [{ ...hill, name: "бугор", radious: 3 }] }), "операция 1 «бугор»: неизвестный ключ «radious»"],
    [plan({ operations: [{ ...hill, name: "бугор", sharp: true }] }), "операция 1 «бугор»: неизвестный ключ «sharp»"],
    [plan({ operations: [{ op: "pad", name: "площадка", area: [[0, 0], [1, 1]] }] }), "операция 1 «площадка»: «area» — список не меньше 3 точек"],
    [plan({ operations: [{ op: "pad", name: "площадка", area: [[0, 0], [1, 1], [2, 2]], sharp: true }] }), "операция 1 «площадка»: «area» — многоугольник без площади"],
    [plan({ operations: [{ op: "channel", name: "ручей", line: [[3, 3], [3, 3]], width: 2, bottom: 0 }] }), "операция 1 «ручей»: «line» — все точки совпадают"],
    [plan({ operations: [{ op: "range", name: "горы", foot: [[0, 0], [9, 0]], side: "up", height: 3, depth: 2 }] }), "операция 1 «горы»: «side»"],
    [plan({ operations: [{ op: "range", name: "горы", foot: [[0, 0], [9, 0]], side: "left", height: 3, depth: 2, roughness: 7 }] }), "операция 1 «горы»: «roughness» не больше 1"],
    [plan({ operations: [{ ...hill, name: "бугор", irregular: 5 }] }), "операция 1 «бугор»: «irregular» не больше 0.9"],
    [plan({ operations: [{ ...hill, name: "бугор", seed: 1.5 }] }), "операция 1 «бугор»: «seed» — целое число"],
    [plan({ operations: [{ op: "channel", name: "ручей", line: [[0, 0], [9, 0]], width: 2, bottom: "низ" }] }), "операция 1 «ручей»: «bottom» — число или два числа"],
    [plan({ operations: [{ ...hill, name: "бугор", radius: 0 }] }), "операция 1 «бугор»: «radius» больше 0"],
    [plan({ operations: [{ op: "noise", name: "волны", amplitude: 1, wavelength: 5, fade: 3 }] }), "операция 1 «волны»: «fade» — только вместе с «area»"],
    [plan({ operations: [{ op: "smooth", name: "мягче", passes: 1e9 }] }), "операция 1 «мягче»: «passes» — целое число от 1 до 50"],
    [plan({ operations: [{ ...hill, name: 42 }] }), "операция 1: «name» — непустой текст"],
    [plan({ operations: [{ ...hill, name: "бугор" }, { ...hill, name: "бугор" }] }), "операция 2 «бугор»: это имя уже есть"],
    [plan({ size: [10] }), "size — два целых числа"],
    [plan({ seed: 2 ** 32 }), "seed — целое число от 0 до 4294967295"],
    [plan({ water: { level: 0, color: "синий" } }), "water — {"],
  ];
  for (const [description, message] of cases) {
    it(message, () => assert.throws(() => buildGrid(description), (error) => error.message.includes(message)));
  }
});

describe("build.mjs", () => {
  it("пишет файл рельефа по пути из описания", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      const result = run(dir, plan());
      assert.equal(result.status, 0, result.stderr);
      const written = readFileSync(join(dir, "terrain.json"), "utf8");
      assert.equal(written, terrainText(buildGrid(plan()), plan().water));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("с covers пишет маски слоёв в terrain/ рядом с файлом рельефа, рельеф — с покрытиями", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      const description = plan({
        covers: [
          { material: "grass" },
          { material: "rock", slope: 34 },
          { material: "moss", rule: "patches", share: 0.3, wavelength: 5, slope: 20 },
        ],
      });
      const result = run(dir, description);
      assert.equal(result.status, 0, result.stderr);
      const grid = buildGrid(description);
      const { layers, masks } = buildCovers(description, grid);
      assert.equal(readFileSync(join(dir, "terrain.json"), "utf8"), terrainText(grid, description.water, layers));
      const written = decodeGrayPng(readFileSync(join(dir, "terrain", "moss.png")));
      assert.deepEqual([written.width, written.height], [64, 48]);
      assert.deepEqual(written.pixels, masks[0].pixels);
      assert.equal(existsSync(join(dir, "terrain", "grass.png")), false, "у первого слоя маски нет");
      assert.equal(existsSync(join(dir, "terrain", "rock.png")), false, "у слоя только со «slope» маски нет");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("перезапись файла рельефа сохраняет его stamps, сколько бы их ни было, и вид файла", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      const stamps = [
        { stamp: "ridge", position: [4, 2.5], size: [10, 6], height: 3 },
        { stamp: "peak", position: [12, 9], size: [6, 6], height: 2.25, rotation: 30 },
      ];
      writeFileSync(join(dir, "terrain.json"), terrainText(new Grid(2, 2), undefined, undefined, stamps));
      const result = run(dir, plan());
      assert.equal(result.status, 0, result.stderr);
      const written = readFileSync(join(dir, "terrain.json"), "utf8");
      assert.equal(written, terrainText(buildGrid(plan()), plan().water, undefined, stamps));
      assert.deepEqual(JSON.parse(written).stamps, stamps);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("файл рельефа без stamps или с пустым списком — после сборки без гор", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      for (const previous of ['{ "heights": [[0]] }', '{ "stamps": [], "heights": [[0]] }']) {
        writeFileSync(join(dir, "terrain.json"), previous);
        assert.equal(run(dir, plan()).status, 0);
        assert.ok(!readFileSync(join(dir, "terrain.json"), "utf8").includes("stamps"));
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("stamps старого файла, что не разбираются, останавливают сборку: файл остаётся как был", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      const cases = [
        ['{ "stamps": [{ "stamp": "ridge" }], "heights": [[0]] }', /stamps → 0: гора/],
        ['{ "stamps": "ridge", "heights": [[0]] }', /stamps — список гор/],
        ["{ stamps: ", /terrain\.json/],
      ];
      for (const [previous, message] of cases) {
        writeFileSync(join(dir, "terrain.json"), previous);
        const result = run(dir, plan());
        assert.equal(result.status, 1);
        assert.match(result.stderr, message);
        assert.equal(readFileSync(join(dir, "terrain.json"), "utf8"), previous);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("при ошибке описания или сломанном JSON файл не пишется и сообщение называет, что не так", () => {
    const dir = mkdtempSync(join(tmpdir(), "location-"));
    try {
      const bad = run(dir, plan({ operations: [{ op: "hill", name: "бугор", at: [1, 1], radius: 2 }] }));
      assert.equal(bad.status, 1);
      assert.match(bad.stderr, /«бугор»: нет обязательного ключа «height»/);
      const broken = run(dir, "{ size: ");
      assert.equal(broken.status, 1);
      assert.match(broken.stderr, /plan\.json/);
      assert.equal(existsSync(join(dir, "terrain.json")), false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("деревня ролевой игры", () => {
  const planPath = fileURLToPath(new URL("../../art/rpg/village/plan.json", import.meta.url));
  const gameDir = fileURLToPath(new URL("../../games/rpg/", import.meta.url));
  const village = JSON.parse(readFileSync(planPath, "utf8"));
  const grid = buildGrid(village);
  const { layers, masks } = buildCovers(village, grid);
  const committed = readFileSync(join(gameDir, "terrain.json"), "utf8").replaceAll("\r\n", "\n");

  it("маски — у каждого слоя с правилом, 512 × 384, четыре точки на клетку сцены", () => {
    assert.equal(masks.length, layers.filter(({ rule }) => rule).length);
    for (const { width, height, pixels } of masks) {
      assert.deepEqual([width, height, pixels.length], [512, 384, 512 * 384]);
    }
  });

  it("файлы в games/rpg — то, что строит описание: рельеф с горами файла и маски те же", () => {
    const { stamps } = JSON.parse(committed);
    assert.ok(stamps.length > 0);
    assert.equal(committed, terrainText(grid, village.water, layers, stamps));
    for (const { mask, pixels } of masks) {
      assert.deepEqual(decodeGrayPng(readFileSync(join(gameDir, mask))).pixels, pixels, mask);
    }
  });

  it("горы файла рельефа и штампы game.json совпадают: каждая гора названа в files.stamps, каждый штамп поставлен и лежит файлом", () => {
    const declared = JSON.parse(readFileSync(join(gameDir, "game.json"), "utf8")).files.stamps;
    const used = new Set(JSON.parse(committed).stamps.map(({ stamp }) => stamp));
    assert.deepEqual([...used].filter((name) => !(name in declared)), []);
    assert.deepEqual(Object.keys(declared).filter((name) => !used.has(name)), []);
    for (const path of Object.values(declared)) assert.ok(existsSync(join(gameDir, path)), path);
  });

  it("список вырезов art/rpg/stamps.json и files.stamps называют одни и те же штампы в одном порядке", () => {
    const list = JSON.parse(readFileSync(fileURLToPath(new URL("../../art/rpg/stamps.json", import.meta.url)), "utf8"));
    const declared = JSON.parse(readFileSync(join(gameDir, "game.json"), "utf8")).files.stamps;
    assert.deepEqual(Object.keys(declared), list.stamps.map(({ name }) => name));
  });

  it("карты цвета и масок камня и осыпи больше нет: они лежат по крутизне", () => {
    for (const file of ["tint", "scree", "rock_moss", "rock"]) assert.equal(existsSync(join(gameDir, "terrain", `${file}.png`)), false, file);
  });
});
