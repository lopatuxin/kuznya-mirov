import { describe, expect, it } from "vitest";
import { beginUndo, createEditSessionState, dirtyFiles, type EditSnapshot } from "./editSession";
import { parseProjectFilePaths } from "./projectFiles";
import { planTerrainEdit } from "./terrainEditing";
import { DEFAULT_WATER, flatTerrainGrid, formatTerrainText, parseTerrainText, terrainTextWithHeights, terrainTextWithWater } from "./terrainFile";

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
  return createEditSessionState({ sceneText: "scene", propertiesText: "props", terrainText: null });
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

  it("мазок без изменений и сцена без размера — действия нет", async () => {
    const same = await planTerrainEdit(sessionWithoutTerrain(), GAME_JSON, NO_FILES, () => null);
    const noScene = await planTerrainEdit(sessionWithoutTerrain(), '{ "files": { "scene": "s.json", "properties": "p.json" } }', NO_FILES, () => "{}");
    expect(same).toBe(null);
    expect(noScene).toBe(null);
  });
});

describe("проект с файлом рельефа", () => {
  const WITH_FILE_JSON = GAME_JSON.replace('"scene": "scene.json",', '"scene": "scene.json",\n    "terrain": "terrain.json",');
  const EXISTING: EditSnapshot = { sceneText: "scene", propertiesText: "props", terrainText: formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: null, covers: null }) };

  it("мазок — одно обычное действие: game.json не меняется, отмена возвращает высоты до мазка", async () => {
    const plan = await planTerrainEdit(createEditSessionState(EXISTING), WITH_FILE_JSON, NO_FILES, (displayed) =>
      terrainTextWithHeights(displayed.terrainText, raisedGrid()),
    );

    expect(plan?.gameJsonText).toBe(WITH_FILE_JSON);
    expect(plan?.state.history).toEqual([EXISTING]);
    expect(beginUndo(plan?.state as NonNullable<typeof plan>["state"])?.candidate).toEqual(EXISTING);
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
