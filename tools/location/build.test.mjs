import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { buildGrid } from "./build.mjs";
import { hundredths, terrainText } from "./files.mjs";
import { Grid } from "./terrain.mjs";

const BUILD = fileURLToPath(new URL("./build.mjs", import.meta.url));

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
