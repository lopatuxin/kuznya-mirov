import { describe, expect, it } from "vitest";
import { beginUndo, createEditSessionState, dirtyFiles, dirtyMaskPaths, markWritten, type EditSnapshot } from "./editSession";
import { NO_MASKS } from "./maskBytes";
import { parseProjectFilePaths } from "./projectFiles";
import { planTerrainEdit, terrainTextAfterPaint } from "./terrainEditing";
import {
  DEFAULT_WATER,
  flatTerrainGrid,
  formatTerrainText,
  parseTerrainText,
  readTerrainMountains,
  terrainTextWithCovers,
  terrainTextWithHeights,
  terrainTextWithMountains,
  terrainTextWithWater,
} from "./terrainFile";

const GAME_JSON = `{
  "name": "Проба",
  "scene": { "width": 2, "height": 1, "camera": { "pitch": 55 } },
  "files": {
    "properties": "properties.json",
    "scene": "scene.json",
    "rules": "rules.json"
  }
}
`;
const SCENE_SIZE = { width: 2, height: 1 };
const NO_FILES = async (): Promise<boolean> => false;

function sessionWithoutTerrain(): ReturnType<typeof createEditSessionState> {
  return createEditSessionState({ sceneText: "scene", propertiesText: "props", terrainText: null, masks: NO_MASKS });
}

function raisedGrid(): ReturnType<typeof flatTerrainGrid> {
  const grid = flatTerrainGrid(SCENE_SIZE);
  grid.heights[7] = 1.5;
  return grid;
}

describe("первый мазок в проекте без рельефа", () => {
  it("заводит terrain.json и дописывает files.terrain в game.json, остальной текст не меняется; обе записи — одно действие", async () => {
    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, (displayed) => terrainTextWithHeights(displayed.terrainText, raisedGrid()));

    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
    expect(plan?.gameJsonText.replace(',\n    "terrain": "terrain.json"', "")).toBe(GAME_JSON);
    expect(plan?.state.history).toHaveLength(1);
    expect(dirtyFiles(plan?.state as NonNullable<typeof plan>["state"]).terrain).toBe(true);
    expect(parseTerrainText(plan?.state.displayed.terrainText ?? "")?.heights[7]).toBe(1.5);
  });

  it("отмена оставляет файл и ключ: высоты — нули, воды нет", async () => {
    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, (displayed) => terrainTextWithHeights(displayed.terrainText, raisedGrid()));
    const undone = beginUndo(plan?.state as NonNullable<typeof plan>["state"]);

    expect(undone?.candidate.terrainText).toBe(formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: null }));
    expect(parseTerrainText(undone?.candidate.terrainText ?? "")?.heights.every((height) => height === 0)).toBe(true);
    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
  });

  it("занятое имя — terrain-2.json; папка scene.json — та же", async () => {
    const isTaken = async (path: string): Promise<boolean> => path === "terrain.json";
    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, isTaken, (displayed) => terrainTextWithHeights(displayed.terrainText, raisedGrid()));
    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain-2.json");

    const inFolder = GAME_JSON.replace('"scene.json"', '"world/scene.json"');
    const folderPlan = await planTerrainEdit(sessionWithoutTerrain(), inFolder, NO_FILES, (displayed) => terrainTextWithHeights(displayed.terrainText, raisedGrid()));
    expect(parseProjectFilePaths(folderPlan?.gameJsonText ?? null)?.terrain).toBe("world/terrain.json");
  });

  it("галочка «Вода» тоже заводит файл: ровная земля и вода −0,5 цвета #3f7fd0", async () => {
    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithWater(displayed.terrainText, sceneSize, DEFAULT_WATER),
    );
    const content = parseTerrainText(plan?.state.displayed.terrainText ?? "");

    expect(content?.water).toEqual({ level: -0.5, color: "#3f7fd0" });
    expect(content?.heights.every((height) => height === 0)).toBe(true);
    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
  });

  it("первая гора тоже заводит terrain.json: ровная земля с горой, отмена оставляет файл без горы", async () => {
    const mountain = { stamp: "beluha", position: [1, 1], size: [2, 1], height: 3 };
    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithMountains(displayed.terrainText, sceneSize, [mountain]),
    );

    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
    expect(readTerrainMountains(plan?.state.displayed.terrainText ?? null)).toEqual([mountain]);
    expect(plan?.state.history).toHaveLength(1);
    const undone = beginUndo(plan?.state as NonNullable<typeof plan>["state"]);
    expect(undone?.candidate.terrainText).toBe(formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: null }));
    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
  });

  it("мазок без изменений и сцена без размера — действия нет", async () => {
    const same = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, () => null);
    const noScene = await planTerrainEdit(sessionWithoutTerrain(), '{ "files": { "scene": "s.json", "properties": "p.json" } }', NO_FILES, () => "{}");
    expect(same).toBe(null);
    expect(noScene).toBe(null);
  });
});

describe("проект с файлом рельефа", () => {
  const WITH_FILE_JSON = GAME_JSON.replace('"scene": "scene.json",', '"scene": "scene.json",\n    "terrain": "terrain.json",');
  const EXISTING: EditSnapshot = { sceneText: "scene", propertiesText: "props", terrainText: formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: null }), masks: NO_MASKS };

  it("мазок — одно обычное действие: game.json не меняется, отмена возвращает высоты до мазка", async () => {
    const plan = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, (displayed) =>
      terrainTextWithHeights(displayed.terrainText, raisedGrid()),
    );

    expect(plan?.gameJsonText).toBe(WITH_FILE_JSON);
    expect(plan?.state.history).toEqual([EXISTING]);
    expect(beginUndo(plan?.state as NonNullable<typeof plan>["state"])?.candidate).toEqual(EXISTING);
  });

  it("гора: правка — одно действие, Ctrl+Z откатывает её вместе с остальным файлом рельефа", async () => {
    const mountain = { stamp: "beluha", position: [1, 1], size: [2, 1], height: 3 };
    const placed = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithMountains(displayed.terrainText, sceneSize, [mountain]),
    );
    const moved = await planTerrainEdit(placed?.state as NonNullable<typeof placed>["state"], WITH_FILE_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithMountains(displayed.terrainText, sceneSize, [{ ...mountain, position: [2, 1] }]),
    );

    expect(placed?.gameJsonText).toBe(WITH_FILE_JSON);
    expect(moved?.state.history).toHaveLength(2);
    expect(beginUndo(moved?.state as NonNullable<typeof moved>["state"])?.candidate.terrainText).toBe(placed?.state.displayed.terrainText);
    expect(beginUndo(placed?.state as NonNullable<typeof placed>["state"])?.candidate.terrainText).toBe(EXISTING.terrainText);
  });

  it("вода: включить и снять — по одному действию, Ctrl+Z возвращает воду до правки", async () => {
    const on = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithWater(displayed.terrainText, sceneSize, DEFAULT_WATER),
    );
    const off = await planTerrainEdit(on?.state as NonNullable<typeof on>["state"], WITH_FILE_JSON, NO_FILES, (displayed, sceneSize) =>
      terrainTextWithWater(displayed.terrainText, sceneSize, null),
    );

    expect(off?.state.history).toHaveLength(2);
    expect(off?.state.displayed.terrainText).toBe(EXISTING.terrainText);
    expect(beginUndo(off?.state as NonNullable<typeof off>["state"])?.candidate.terrainText).toBe(on?.state.displayed.terrainText);
  });
});

describe("покраска — действие с масками («Покраска», требования 12, 15–16)", () => {
  const WITH_FILE_JSON = GAME_JSON.replace('"scene": "scene.json",', '"scene": "scene.json",\n    "terrain": "terrain.json",');
  const FLAT_WITH_GRASS = formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: [{ material: "grass" }] });
  const EXISTING: EditSnapshot = { sceneText: "scene", propertiesText: "props", terrainText: FLAT_WITH_GRASS, masks: NO_MASKS };
  const COVERS = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }];
  const PAINTED = { "terrain/rock.png": { width: 8, height: 4, pixels: new Uint8Array(32).fill(7) } };
  const withCovers = (displayed: EditSnapshot, sceneSize: typeof SCENE_SIZE): string | null => terrainTextWithCovers(displayed.terrainText, sceneSize, COVERS);

  it("мазок, что положил слой: рельеф и маска — одно действие, маска не записана, отмена убирает слой и маску", async () => {
    const plan = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, withCovers, PAINTED);

    const state = plan?.state as NonNullable<typeof plan>["state"];
    expect(plan?.gameJsonText).toBe(WITH_FILE_JSON);
    expect(parseTerrainText(state.displayed.terrainText ?? "")?.covers).toEqual(COVERS);
    expect(state.displayed.masks).toEqual(PAINTED);
    expect(dirtyMaskPaths(state)).toEqual(["terrain/rock.png"]);
    expect(state.history).toEqual([EXISTING]);
    expect(beginUndo(state)?.candidate).toEqual(EXISTING);
  });

  it("слои те же, байты маски другие — тоже действие: текст рельефа не меняется, маска пишется", async () => {
    const painted = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, withCovers, PAINTED);
    const written = markWritten(painted?.state as NonNullable<typeof painted>["state"]);
    const repainted = { "terrain/rock.png": { width: 8, height: 4, pixels: new Uint8Array(32).fill(9) } };

    const plan = await planTerrainEdit(written, WITH_FILE_JSON, NO_FILES, withCovers, repainted);

    const state = plan?.state as NonNullable<typeof plan>["state"];
    expect(state.displayed.terrainText).toBe(written.displayed.terrainText);
    expect(dirtyFiles(state).terrain).toBe(false);
    expect(dirtyMaskPaths(state)).toEqual(["terrain/rock.png"]);
    expect(beginUndo(state)?.candidate.masks).toEqual(PAINTED);
  });

  it("мазок только по маске: текст рельефа с нестандартным числом остаётся байт в байт, пишется одна маска", async () => {
    const handWritten = formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: COVERS }).replace("0, 0", "1.234, 0");
    expect(handWritten).toContain("1.234");
    const withRock = (displayed: EditSnapshot, sceneSize: typeof SCENE_SIZE): string | null => terrainTextAfterPaint(displayed.terrainText, sceneSize, COVERS);
    const onDisk = createEditSessionState({ ...EXISTING, terrainText: handWritten, masks: PAINTED });
    const repainted = { "terrain/rock.png": { width: 8, height: 4, pixels: new Uint8Array(32).fill(9) } };

    const plan = await planTerrainEdit(onDisk, WITH_FILE_JSON, NO_FILES, withRock, repainted);

    const state = plan?.state as NonNullable<typeof plan>["state"];
    expect(state.displayed.terrainText).toBe(handWritten);
    expect(dirtyFiles(state).terrain).toBe(false);
    expect(dirtyMaskPaths(state)).toEqual(["terrain/rock.png"]);
  });

  it("слои изменились — текст рельефа пересобран со слоями, слои те же — прежний текст", () => {
    const handWritten = FLAT_WITH_GRASS.replace("0, 0", "1.234, 0");

    expect(terrainTextAfterPaint(handWritten, SCENE_SIZE, [{ material: "grass" }])).toBe(handWritten);
    expect(terrainTextAfterPaint(handWritten, SCENE_SIZE, COVERS)).toBe(terrainTextWithCovers(handWritten, SCENE_SIZE, COVERS));
    expect(terrainTextAfterPaint(null, SCENE_SIZE, COVERS)).toBe(terrainTextWithCovers(null, SCENE_SIZE, COVERS));
  });

  it("ни слои, ни маски не изменились — действия нет", async () => {
    const same = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, (displayed) => displayed.terrainText);

    expect(same).toBeNull();
  });

  it("новые маски ложатся поверх прежних, остальные остаются", async () => {
    const base: EditSnapshot = { ...EXISTING, masks: { "terrain/scree.png": { width: 1, height: 1, pixels: Uint8Array.of(3) } } };

    const plan = await planTerrainEdit(createEditSessionState(base), WITH_FILE_JSON, NO_FILES, withCovers, PAINTED);

    expect(Object.keys(plan?.state.displayed.masks ?? {}).sort()).toEqual(["terrain/rock.png", "terrain/scree.png"]);
  });

  it("первый мазок в проекте без рельефа заводит terrain.json и files.terrain; отмена оставляет их, но слоёв нет", async () => {
    const grassOnly = (displayed: EditSnapshot, sceneSize: typeof SCENE_SIZE): string | null => terrainTextWithCovers(displayed.terrainText, sceneSize, [{ material: "grass" }]);

    const plan = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, grassOnly);

    const state = plan?.state as NonNullable<typeof plan>["state"];
    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.terrain).toBe("terrain.json");
    expect(parseTerrainText(state.displayed.terrainText ?? "")?.covers).toEqual([{ material: "grass" }]);
    expect(parseTerrainText(state.displayed.terrainText ?? "")?.water).toBeNull();
    const undone = beginUndo(state);
    expect(parseTerrainText(undone?.candidate.terrainText ?? "")?.covers).toBeNull();
    expect(parseTerrainText(undone?.candidate.terrainText ?? "")?.heights.every((height) => height === 0)).toBe(true);
  });
});
