import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_WATER,
  differsInHundredths,
  flatTerrainGrid,
  formatTerrainText,
  parseTerrainText,
  readTerrainCovers,
  readTerrainImprints,
  readTerrainWater,
  terrainTextWithCovers,
  terrainTextWithHeights,
  terrainTextWithImprints,
  terrainTextWithWater,
  type ImprintEntry,
  type TerrainCoverLayer,
  type TerrainGrid,
} from "./terrainFile";

const RPG_TERRAIN = readFileSync(new URL("../../../games/rpg/terrain.json", import.meta.url), "utf8");

function grid(rows: number[][]): TerrainGrid {
  return { columns: rows[0]?.length ?? 0, rows: rows.length, heights: Float64Array.from(rows.flat()) };
}

describe("formatTerrainText", () => {
  it("water первой строкой, строка сетки на строку файла, числа через запятую с пробелом", () => {
    const text = formatTerrainText({ ...grid([[0, 1.5], [-2.35, 3]]), water: { level: -2.3, color: "#3f7fd0" }, covers: null });
    expect(text).toBe(
      ['{', '  "water": { "level": -2.3, "color": "#3f7fd0" },', '  "heights": [', "    [0, 1.5],", "    [-2.35, 3]", "  ]", "}", ""].join("\n"),
    );
  });

  it("без воды ключа water нет", () => {
    const text = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null });
    expect(text).toBe(['{', '  "heights": [', "    [0, 0]", "  ]", "}", ""].join("\n"));
  });

  it("числа до сотых без лишних нулей, -0 пишется 0", () => {
    const text = formatTerrainText({ ...grid([[1.004, 1.006, 1.5, -0, -0.004, 2.999, 0.07]]), water: null, covers: null });
    expect(text).toContain("[1, 1.01, 1.5, 0, 0, 3, 0.07]");
  });
});

describe("файл рельефа ролевой игры", () => {
  it("прочитанный и записанный без изменений совпадает с собой", () => {
    const content = parseTerrainText(RPG_TERRAIN);
    expect(content).not.toBe(null);
    expect(terrainTextWithHeights(RPG_TERRAIN, content as TerrainGrid)).toBe(RPG_TERRAIN);
  });

  it("вода читается так, как лежит в файле, а без неё — её нет", () => {
    // Воды в рельефе может и не быть — тогда её нет и после чтения.
    expect(readTerrainWater(RPG_TERRAIN)).toEqual(JSON.parse(RPG_TERRAIN).water ?? null);
  });
});

describe("parseTerrainText", () => {
  it("текст, что не разбирается или не сетка, — null", () => {
    expect(parseTerrainText("{")).toBe(null);
    expect(parseTerrainText('{ "heights": [[0, 0], [0]] }')).toBe(null);
    expect(parseTerrainText('{ "heights": [[0, "a"]] }')).toBe(null);
    expect(parseTerrainText("[]")).toBe(null);
  });

  it("воды нет — null; сетка идёт строками сверху вниз", () => {
    const content = parseTerrainText('{ "heights": [[1, 2], [3, 4]] }');
    expect(content?.water).toBe(null);
    expect(Array.from(content?.heights ?? [])).toEqual([1, 2, 3, 4]);
    expect(readTerrainWater(null)).toBe(null);
  });
});

describe("правка текста рельефа", () => {
  it("мазок: высоты новые, вода и перенос строки прежние", () => {
    const before = formatTerrainText({ ...grid([[0, 0]]), water: { level: -1, color: "#112233" }, covers: null }, "\r\n");
    const after = terrainTextWithHeights(before, grid([[0.5, 2]]));
    expect(after).toBe(formatTerrainText({ ...grid([[0.5, 2]]), water: { level: -1, color: "#112233" }, covers: null }, "\r\n"));
  });

  it("мазок без файла — файл без воды", () => {
    expect(terrainTextWithHeights(null, grid([[1]]))).toBe(formatTerrainText({ ...grid([[1]]), water: null, covers: null }));
  });

  it("вода включается на уровне -0,5 цвета #3f7fd0 и уходит из файла; высоты остаются", () => {
    const before = formatTerrainText({ ...grid([[0, 1.5]]), water: null, covers: null });
    const withWater = terrainTextWithWater(before, { width: 1, height: 1 }, DEFAULT_WATER);
    expect(withWater).toBe(formatTerrainText({ ...grid([[0, 1.5]]), water: { level: -0.5, color: "#3f7fd0" }, covers: null }));
    expect(terrainTextWithWater(withWater, { width: 1, height: 1 }, null)).toBe(before);
  });

  it("вода в проекте без файла — ровная земля нужного размера: (2 × высота + 1) × (2 × ширина + 1) чисел", () => {
    const text = terrainTextWithWater(null, { width: 3, height: 2 }, DEFAULT_WATER) as string;
    const content = parseTerrainText(text);
    expect(content?.columns).toBe(7);
    expect(content?.rows).toBe(5);
    expect(content?.heights.every((height) => height === 0)).toBe(true);
    expect(content?.water).toEqual(DEFAULT_WATER);
  });

  it("уровень воды — до сотых", () => {
    const text = terrainTextWithWater(null, { width: 1, height: 1 }, { level: -1.234, color: "#000000" }) as string;
    expect(readTerrainWater(text)?.level).toBe(-1.23);
  });

  it("файл, что не разбирается, — править нечего", () => {
    expect(terrainTextWithWater("{", { width: 1, height: 1 }, DEFAULT_WATER)).toBe(null);
  });
});

describe("flatTerrainGrid / differsInHundredths", () => {
  it("ровная земля — нули (2 × высота + 1) × (2 × ширина + 1)", () => {
    const flat = flatTerrainGrid({ width: 32, height: 24 });
    expect(flat.columns).toBe(65);
    expect(flat.rows).toBe(49);
    expect(flat.heights).toHaveLength(65 * 49);
  });

  it("высоты, что округляются до одной сотой, не отличаются", () => {
    expect(differsInHundredths([1, 2], [1.001, 1.998])).toBe(false);
    expect(differsInHundredths([1, 2], [1.01, 2])).toBe(true);
  });
});

describe("покрытия covers", () => {
  const COVERS = [{ material: "grass" }, { material: "earth", mask: "terrain/earth.png" }];
  const WATER = { level: -1, color: "#112233" };
  const textWithCovers = formatTerrainText({ ...grid([[0, 0]]), water: WATER, covers: COVERS });

  it("covers — после water, по слою на строку; маска — у слоя, у которого она есть", () => {
    expect(textWithCovers).toBe(
      [
        "{",
        '  "water": { "level": -1, "color": "#112233" },',
        '  "covers": [',
        '    { "material": "grass" },',
        '    { "material": "earth", "mask": "terrain/earth.png" }',
        "  ],",
        '  "heights": [',
        "    [0, 0]",
        "  ]",
        "}",
        "",
      ].join("\n"),
    );
    expect(parseTerrainText(textWithCovers)?.covers).toEqual(COVERS);
  });

  it("мазок оставляет covers как были, меняя только высоты", () => {
    const after = terrainTextWithHeights(textWithCovers, grid([[0.5, 2]]));
    expect(after).toBe(formatTerrainText({ ...grid([[0.5, 2]]), water: WATER, covers: COVERS }));
  });

  it("правка воды оставляет covers и высоты как были, в том числе при выключении воды", () => {
    const withoutWater = terrainTextWithWater(textWithCovers, { width: 1, height: 1 }, null);
    expect(withoutWater).toBe(formatTerrainText({ ...grid([[0, 0]]), water: null, covers: COVERS }));
    expect(terrainTextWithWater(withoutWater, { width: 1, height: 1 }, WATER)).toBe(textWithCovers);
  });

  it("файл без covers при мазке и правке воды covers не получает; новый файл — тоже", () => {
    const plain = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null });
    expect(terrainTextWithHeights(plain, grid([[1, 1]]))).not.toContain("covers");
    expect(terrainTextWithWater(plain, { width: 1, height: 1 }, WATER)).not.toContain("covers");
    expect(terrainTextWithHeights(null, grid([[1]]))).not.toContain("covers");
    expect(terrainTextWithWater(null, { width: 1, height: 1 }, WATER)).not.toContain("covers");
  });

  it("перенос строки CRLF сохраняется и у covers", () => {
    const crlf = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: COVERS }, "\r\n");
    expect(terrainTextWithHeights(crlf, grid([[3, 3]]))).toBe(formatTerrainText({ ...grid([[3, 3]]), water: null, covers: COVERS }, "\r\n"));
  });

  it("слой с порогом крутизны: slope перед маской, число как есть; читается обратно", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", slope: 34 }, { material: "scree", slope: 22.5, mask: "terrain/scree.png" }];
    const text = formatTerrainText({ ...grid([[0, 0]]), water: null, covers });
    expect(text).toContain('    { "material": "rock", "slope": 34 },');
    expect(text).toContain('    { "material": "scree", "slope": 22.5, "mask": "terrain/scree.png" }');
    expect(parseTerrainText(text)?.covers).toEqual(covers);
  });
});

const IMPRINTS: ImprintEntry[] = [
  { stamp: "beluha", position: [8, 0], size: [38, 32], height: 15 },
  { stamp: "chuya", position: [104.004, 4], size: [40, 30.125], height: 18, rotation: 345 },
  { stamp: "taganai", position: [124, 52], size: [34, 16], height: 8, rotation: 0 },
];

describe("отпечатки stamps", () => {
  const WATER = { level: -1, color: "#112233" };
  const COVERS = [{ material: "grass" }, { material: "rock", slope: 34 }];
  const textWithImprints = formatTerrainText({ ...grid([[0, 0]]), water: WATER, covers: COVERS, tint: "terrain/tint.png", stamps: IMPRINTS });
  const plainText = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null });

  it("stamps — после tint, по отпечатку на строку, перед heights; числа до сотых, rotation — если не 0", () => {
    expect(textWithImprints).toBe(
      [
        "{",
        '  "water": { "level": -1, "color": "#112233" },',
        '  "covers": [',
        '    { "material": "grass" },',
        '    { "material": "rock", "slope": 34 }',
        "  ],",
        '  "tint": "terrain/tint.png",',
        '  "stamps": [',
        '    { "stamp": "beluha", "position": [8, 0], "size": [38, 32], "height": 15 },',
        '    { "stamp": "chuya", "position": [104, 4], "size": [40, 30.13], "height": 18, "rotation": 345 },',
        '    { "stamp": "taganai", "position": [124, 52], "size": [34, 16], "height": 8 }',
        "  ],",
        '  "heights": [',
        "    [0, 0]",
        "  ]",
        "}",
        "",
      ].join("\n"),
    );
  });

  it("пустой список и null — ключа stamps нет", () => {
    expect(formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null, stamps: [] })).not.toContain("stamps");
    expect(formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null, stamps: null })).not.toContain("stamps");
  });

  it("отпечатки читаются как лежат в файле; нет ключа — нет отпечатков", () => {
    expect(readTerrainImprints(textWithImprints)).toHaveLength(3);
    expect(readTerrainImprints(textWithImprints)[1]).toEqual({ stamp: "chuya", position: [104, 4], size: [40, 30.13], height: 18, rotation: 345 });
    expect(readTerrainImprints(plainText)).toEqual([]);
    expect(readTerrainImprints(null)).toEqual([]);
    expect(readTerrainImprints("{")).toEqual([]);
  });

  it("вдавливающий отпечаток пишется с отрицательной высотой и читается обратно", () => {
    const ravine: ImprintEntry = { stamp: "ravine", position: [10, 8], size: [20, 10], height: -3 };
    const text = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null, stamps: [ravine] });
    expect(text).toContain('{ "stamp": "ravine", "position": [10, 8], "size": [20, 10], "height": -3 }');
    expect(readTerrainImprints(text)).toEqual([ravine]);
  });

  it("мазок и правка воды сохраняют stamps, tint и covers", () => {
    expect(terrainTextWithHeights(textWithImprints, grid([[0.5, 2]]))).toBe(
      formatTerrainText({ ...grid([[0.5, 2]]), water: WATER, covers: COVERS, tint: "terrain/tint.png", stamps: readTerrainImprints(textWithImprints) }),
    );
    const withoutWater = terrainTextWithWater(textWithImprints, { width: 1, height: 1 }, null);
    expect(readTerrainImprints(withoutWater)).toEqual(readTerrainImprints(textWithImprints));
    expect(withoutWater).toContain('"tint"');
  });

  it("правка отпечатков сохраняет heights, covers, tint и воду", () => {
    const after = terrainTextWithImprints(textWithImprints, { width: 1, height: 1 }, [{ stamp: "beluha", position: [1, 2], size: [3, 4], height: 5 }]) as string;
    const content = parseTerrainText(after);
    expect(content?.water).toEqual(WATER);
    expect(content?.covers).toEqual(COVERS);
    expect(content?.tint).toBe("terrain/tint.png");
    expect(Array.from(content?.heights ?? [])).toEqual([0, 0]);
    expect(content?.stamps).toEqual([{ stamp: "beluha", position: [1, 2], size: [3, 4], height: 5 }]);
  });

  it("последний отпечаток убран — ключ stamps уходит из файла", () => {
    const after = terrainTextWithImprints(textWithImprints, { width: 1, height: 1 }, []) as string;
    expect(after).not.toContain("stamps");
    expect(after).toBe(formatTerrainText({ ...grid([[0, 0]]), water: WATER, covers: COVERS, tint: "terrain/tint.png" }));
  });

  it("первый отпечаток в проекте без файла — ровная земля нужного размера без воды и покрытий", () => {
    const text = terrainTextWithImprints(null, { width: 3, height: 2 }, [IMPRINTS[0] as ImprintEntry]) as string;
    const content = parseTerrainText(text);
    expect(content?.columns).toBe(7);
    expect(content?.rows).toBe(5);
    expect(content?.water).toBe(null);
    expect(content?.covers).toBe(null);
    expect(content?.stamps).toEqual([IMPRINTS[0]]);
  });

  it("файл, что не разбирается, — править нечего; перенос строки сохраняется", () => {
    expect(terrainTextWithImprints("{", { width: 1, height: 1 }, IMPRINTS)).toBe(null);
    const crlf = formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null, stamps: IMPRINTS }, "\r\n");
    expect(terrainTextWithImprints(crlf, { width: 1, height: 1 }, [])).toBe(formatTerrainText({ ...grid([[0, 0]]), water: null, covers: null }, "\r\n"));
  });

  it("набранное в свойствах пишется как есть: ошибку назовёт движок", () => {
    const text = formatTerrainText({
      ...grid([[0, 0]]),
      water: null,
      covers: null,
      stamps: [{ stamp: "beluha", position: "abc", size: [3, "x"], height: [1.234, 2], rotation: null, extra: true }],
    });
    expect(text).toContain('{ "stamp": "beluha", "position": "abc", "size": [3,"x"], "height": [1.23, 2], "rotation": null, "extra": true }');
  });
});

describe("покраска — текст рельефа со слоями («Покраска», требования 13, 15)", () => {
  const LAYERS: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", slope: 44, mask: "terrain/rock.png" }];
  const SIZE = { width: 1, height: 1 };
  const BELUHA: ImprintEntry[] = [{ stamp: "beluha", position: [8, 0], size: [38, 32], height: 15 }];

  it("меняет только слои: высоты, вода, отпечатки и карта цвета остаются, слой с порогом и маской — одной строкой", () => {
    const content = { ...grid([[0, 1.25], [2, 3]]), water: DEFAULT_WATER, covers: [{ material: "grass" }], tint: "terrain/tint.png", stamps: BELUHA };
    const text = formatTerrainText(content);

    const painted = terrainTextWithCovers(text, SIZE, LAYERS);

    expect(painted).toContain('    { "material": "rock", "slope": 44, "mask": "terrain/rock.png" }');
    const parsed = parseTerrainText(painted ?? "");
    expect(parsed?.covers).toEqual(LAYERS);
    expect(parsed?.water).toEqual(DEFAULT_WATER);
    expect(parsed?.tint).toBe("terrain/tint.png");
    expect(parsed?.stamps).toEqual(BELUHA);
    expect(Array.from(parsed?.heights ?? [])).toEqual([0, 1.25, 2, 3]);
  });

  it("слои те же — текст тот же", () => {
    const text = formatTerrainText({ ...grid([[0, 1], [2, 3]]), water: null, covers: LAYERS });

    expect(terrainTextWithCovers(text, SIZE, LAYERS)).toBe(text);
  });

  it("перенос строки файла сохраняется", () => {
    const text = formatTerrainText({ ...grid([[0, 1], [2, 3]]), water: null, covers: null }, "\r\n");

    expect(terrainTextWithCovers(text, SIZE, LAYERS)?.includes("\r\n")).toBe(true);
  });

  it("без файла — ровная земля сцены без воды со слоями", () => {
    const text = terrainTextWithCovers(null, SIZE, [{ material: "grass" }]);

    const parsed = parseTerrainText(text ?? "");
    expect(parsed?.heights.length).toBe(9);
    expect(parsed?.heights.every((height) => height === 0)).toBe(true);
    expect(parsed?.water).toBeNull();
    expect(parsed?.covers).toEqual([{ material: "grass" }]);
  });

  it("файл, что не разбирается, — null: править нечего", () => {
    expect(terrainTextWithCovers("{", SIZE, LAYERS)).toBeNull();
  });

  it("readTerrainCovers: слои файла; файла нет, не разбирается или без covers — null", () => {
    expect(readTerrainCovers(formatTerrainText({ ...grid([[0]]), water: null, covers: LAYERS }))).toEqual(LAYERS);
    expect(readTerrainCovers(formatTerrainText({ ...grid([[0]]), water: null, covers: null }))).toBeNull();
    expect(readTerrainCovers("{")).toBeNull();
    expect(readTerrainCovers(null)).toBeNull();
  });
});
