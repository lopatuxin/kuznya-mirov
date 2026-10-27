import { describe, expect, it } from "vitest";
import { beginUndo, createEditSessionState, type EditSnapshot } from "./editSession";
import { NO_MASKS } from "./maskBytes";
import { planMotionCreation } from "./motionsEditing";
import { readMotionNames } from "./motionsFile";
import { chooseMotionsFilePath, parseProjectFilePaths } from "./projectFiles";

const GAME_JSON = `{
  "name": "Тест",
  "files": {
    "scene": "scene.json",
    "properties": "properties.json"
  }
}`;
const WITHOUT_FILE: EditSnapshot = { sceneText: "scene", propertiesText: "props", terrainText: null, motionsText: null, masks: NO_MASKS };
const NO_FILES = async (): Promise<boolean> => false;
const JOG = { name: "hero_jog", model: "hero", clip: "Jog" };

describe("планируемое новое движение", () => {
  it("в игре без motions.json заводит файл и дописывает files.motions в game.json", async () => {
    const plan = await planMotionCreation(createEditSessionState(WITHOUT_FILE), GAME_JSON, NO_FILES, JOG);

    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.motions).toBe("motions.json");
    expect(readMotionNames(plan?.state.displayed.motionsText ?? null)).toEqual(["hero_jog"]);
    expect(JSON.parse(plan?.gameJsonText ?? "").files).toEqual({ scene: "scene.json", properties: "properties.json", motions: "motions.json" });
  });

  it("отмена первого движения возвращает пустую таблицу, а не «файла нет»", async () => {
    const plan = await planMotionCreation(createEditSessionState(WITHOUT_FILE), GAME_JSON, NO_FILES, JOG);

    const undone = beginUndo(plan?.state as NonNullable<typeof plan>["state"]);

    expect(undone?.candidate.motionsText).toBe("{}\n");
  });

  it("занятое имя файла не затирается: берётся motions-2.json", async () => {
    const plan = await planMotionCreation(createEditSessionState(WITHOUT_FILE), GAME_JSON, async (path) => path === "motions.json", JOG);

    expect(parseProjectFilePaths(plan?.gameJsonText ?? null)?.motions).toBe("motions-2.json");
  });

  it("в игре, что уже назвала файл, game.json не меняется; следующее движение — обычное действие с отменой", async () => {
    const named = GAME_JSON.replace('"scene": "scene.json",', '"scene": "scene.json",\n    "motions": "anim/motions.json",');
    const first = await planMotionCreation(createEditSessionState(WITHOUT_FILE), named, NO_FILES, JOG);
    expect(first?.gameJsonText).toBe(named);

    const second = await planMotionCreation(first?.state as NonNullable<typeof first>["state"], named, NO_FILES, { name: "hero_roll", model: "hero", clip: "Roll" });
    expect(readMotionNames(second?.state.displayed.motionsText ?? null)).toEqual(["hero_jog", "hero_roll"]);
    expect(beginUndo(second?.state as NonNullable<typeof second>["state"])?.candidate.motionsText).toBe(first?.state.displayed.motionsText);
  });

  it("занятое или пустое имя — действия нет", async () => {
    const first = await planMotionCreation(createEditSessionState(WITHOUT_FILE), GAME_JSON, NO_FILES, JOG);

    expect(await planMotionCreation(first?.state as NonNullable<typeof first>["state"], first?.gameJsonText ?? "", NO_FILES, JOG)).toBeNull();
    expect(await planMotionCreation(createEditSessionState(WITHOUT_FILE), GAME_JSON, NO_FILES, { ...JOG, name: "" })).toBeNull();
    expect(await planMotionCreation(createEditSessionState(WITHOUT_FILE), "{", NO_FILES, JOG)).toBeNull();
  });
});

describe("chooseMotionsFilePath", () => {
  it("motions.json, а занятое имя — со следующим номером", async () => {
    expect(await chooseMotionsFilePath(NO_FILES)).toBe("motions.json");
    expect(await chooseMotionsFilePath(async (path) => path !== "motions-3.json")).toBe("motions-3.json");
  });
});
