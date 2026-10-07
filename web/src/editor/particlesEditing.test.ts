import { describe, expect, it } from "vitest";
import { beginUndo, createEditSessionState, dirtyFiles, type EditSnapshot } from "./editSession";
import { NO_MASKS } from "./maskBytes";
import {
  EMPTY_PARTICLES_TEXT,
  particleCopyChange,
  particleDeleteChange,
  particleKindNameError,
  particleRenameChange,
  particleSourceChange,
  particleValueChange,
  planParticlesEdit,
} from "./particlesEditing";
import { readParticleKinds } from "./particlesFile";
import { builtInParticleFields } from "./particlePresets";

const GAME_JSON = '{\n  "name": "Тест",\n  "files": {\n    "scene": "scene.json",\n    "properties": "properties.json"\n  }\n}\n';
const GAME_JSON_WITH_FILE = GAME_JSON.replace('"properties": "properties.json"', '"properties": "properties.json",\n    "particles": "particles.json"');
const SCENE = '{ "objects": [{ "position": [1, 1], "size": [1, 1], "particles": "дым" }, { "position": [2, 1], "size": [1, 1], "particles": "искры" }] }';
const FILE = '{\n  "дым": { "image": "puff", "rate": 6, "lifetime": 2, "size": 1 },\n  "искры": { "image": "spark", "rate": 9, "lifetime": 1, "size": 0.2 }\n}\n';
const NO_FILES = async (): Promise<boolean> => false;

const WITH_FILE: EditSnapshot = { sceneText: SCENE, propertiesText: "{}", terrainText: null, particlesText: FILE, masks: NO_MASKS };
const WITHOUT_FILE: EditSnapshot = { ...WITH_FILE, particlesText: null };

describe("planParticlesEdit: проект без файла видов", () => {
  it("первая правка готового вида заводит particles.json с ним и дописывает files.particles в game.json, отмена возвращает пустую таблицу", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITHOUT_FILE), GAME_JSON, NO_FILES, (displayed) => particleValueChange(displayed, "листья", "rate", 3));

    expect(plan).not.toBeNull();
    expect(plan?.gameJsonText).toBe(GAME_JSON_WITH_FILE);
    const kinds = readParticleKinds(plan?.state.displayed.particlesText ?? null);
    expect(kinds.map((kind) => kind.name)).toEqual(["листья"]);
    expect(kinds[0]?.fields).toEqual({ ...builtInParticleFields("листья"), rate: 3 });
    expect(plan?.selected).toBe("листья");
    expect(plan?.state.displayed.particlesText?.endsWith("}\n")).toBe(true);
    expect(dirtyFiles(plan?.state as NonNullable<typeof plan>["state"]).particles).toBe(true);
    expect(beginUndo(plan?.state as NonNullable<typeof plan>["state"])?.candidate.particlesText).toBe(EMPTY_PARTICLES_TEXT);
  });

  it("имя занято — particles-2.json", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITHOUT_FILE), GAME_JSON, async (path) => path === "particles.json", (displayed) => particleValueChange(displayed, "дым", "rate", 3));

    expect(plan?.gameJsonText).toContain('"particles": "particles-2.json"');
  });

  it("game.json без путей сцены — правки нет", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITHOUT_FILE), "{}", NO_FILES, (displayed) => particleValueChange(displayed, "дым", "rate", 3));

    expect(plan).toBeNull();
  });
});

describe("planParticlesEdit: проект с файлом видов", () => {
  it("обычное действие: game.json не меняется, отмена возвращает прежний файл", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITH_FILE), GAME_JSON_WITH_FILE, NO_FILES, (displayed) => particleValueChange(displayed, "дым", "rate", 9));

    expect(plan?.gameJsonText).toBe(GAME_JSON_WITH_FILE);
    expect(plan?.state.displayed.particlesText).toBe(FILE.replace('"rate": 6', '"rate": 9'));
    expect(beginUndo(plan?.state as NonNullable<typeof plan>["state"])?.candidate.particlesText).toBe(FILE);
  });

  it("то же значение — действия нет", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITH_FILE), GAME_JSON_WITH_FILE, NO_FILES, (displayed) => particleValueChange(displayed, "дым", "rate", 6));

    expect(plan).toBeNull();
  });

  it("правка вида, которого нет, — действия нет", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITH_FILE), GAME_JSON_WITH_FILE, NO_FILES, (displayed) => particleValueChange(displayed, "нет", "rate", 9));

    expect(plan).toBeNull();
  });
});

describe("действия вкладки", () => {
  it("новый ключ дописывается последним в свой вид, пустое значение убирает ключ", () => {
    const added = particleValueChange(WITH_FILE, "дым", "gravity", 2);
    expect(added?.snapshot.particlesText).toBe(FILE.replace('"size": 1 }', '"size": 1, "gravity": 2 }'));

    const removed = particleValueChange(WITH_FILE, "дым", "lifetime", undefined);
    expect(removed?.snapshot.particlesText).toBe(FILE.replace('"lifetime": 2, ', ""));
  });

  it("копия — последней в файле, «<имя>-копия», и становится выбранной", () => {
    const change = particleCopyChange(WITH_FILE, "дым");

    expect(change?.selected).toBe("дым-копия");
    const kinds = readParticleKinds(change?.snapshot.particlesText ?? null);
    expect(kinds.map((kind) => kind.name)).toEqual(["дым", "искры", "дым-копия"]);
    expect(kinds[2]?.fields).toEqual(kinds[0]?.fields);
  });

  it("копия копии получает «-копия-2», если «-копия» занято", () => {
    const first = particleCopyChange(WITH_FILE, "дым");
    const second = particleCopyChange(first?.snapshot as EditSnapshot, "дым");

    expect(second?.selected).toBe("дым-копия-2");
  });

  it("источник перетащенного готового вида: вид записан в файл целиком, источник — в конец objects, одним снимком", () => {
    const source = { position: [5, 5], size: [1, 1], particles: "листья" };
    const change = particleSourceChange(WITH_FILE, "листья", source);

    const kinds = readParticleKinds(change?.snapshot.particlesText ?? null);
    expect(kinds.map((kind) => kind.name)).toEqual(["дым", "искры", "листья"]);
    expect(kinds[2]?.fields).toEqual(builtInParticleFields("листья"));
    const objects = (JSON.parse(change?.snapshot.sceneText ?? "{}") as { objects: unknown[] }).objects;
    expect(objects.at(-1)).toEqual(source);
    expect(change?.selected).toBe("листья");
  });

  it("источник вида, который уже в файле, файл видов не меняет", () => {
    const change = particleSourceChange(WITH_FILE, "дым", { position: [5, 5], size: [1, 1], particles: "дым" });

    expect(change?.snapshot.particlesText).toBe(FILE);
    expect((JSON.parse(change?.snapshot.sceneText ?? "{}") as { objects: unknown[] }).objects).toHaveLength(3);
  });

  it("источник неизвестного вида — действия нет", () => {
    expect(particleSourceChange(WITH_FILE, "туман", { position: [5, 5], size: [1, 1], particles: "туман" })).toBeNull();
  });

  it("копия готового вида, которого нет в файле, — копия заготовки", () => {
    const change = particleCopyChange(WITH_FILE, "листья");

    expect(change?.selected).toBe("листья-копия");
    expect(readParticleKinds(change?.snapshot.particlesText ?? null)[2]?.fields).toEqual(builtInParticleFields("листья"));
  });

  it("удаление убирает вид из файла и particles у его источников в scene.json одним снимком", () => {
    const change = particleDeleteChange(WITH_FILE, "дым");

    expect(readParticleKinds(change?.snapshot.particlesText ?? null).map((kind) => kind.name)).toEqual(["искры"]);
    const objects = (JSON.parse(change?.snapshot.sceneText ?? "{}") as { objects: Record<string, unknown>[] }).objects;
    expect(objects.map((object) => object.particles)).toEqual([undefined, "искры"]);
    expect(change?.selected).toBe("искры");
  });

  it("удаление последнего вида — выбора нет, файл остаётся с пустой таблицей", () => {
    const only = particleDeleteChange({ ...WITH_FILE, particlesText: '{ "дым": { "rate": 1 } }' }, "дым");

    expect(only?.snapshot.particlesText).toBe("{}");
    expect(only?.selected).toBeNull();
  });

  it("переименование — вид на своём месте в файле и particles у источников в scene.json одним снимком", () => {
    const change = particleRenameChange(WITH_FILE, "дым", "туман");

    expect(change?.snapshot.particlesText).toBe(FILE.replace('"дым"', '"туман"'));
    const objects = (JSON.parse(change?.snapshot.sceneText ?? "{}") as { objects: Record<string, unknown>[] }).objects;
    expect(objects.map((object) => object.particles)).toEqual(["туман", "искры"]);
    expect(change?.selected).toBe("туман");
  });

  it("одна отмена возвращает оба файла", async () => {
    const plan = await planParticlesEdit(createEditSessionState(WITH_FILE), GAME_JSON_WITH_FILE, NO_FILES, (displayed) => particleRenameChange(displayed, "дым", "туман"));
    const undone = beginUndo(plan?.state as NonNullable<typeof plan>["state"]);

    expect(undone?.candidate.particlesText).toBe(FILE);
    expect(undone?.candidate.sceneText).toBe(SCENE);
  });
});

describe("particleKindNameError", () => {
  const names = ["дым", "искры"];

  it("пустое имя и имя другого вида — ошибка", () => {
    expect(particleKindNameError("", "дым", names)).toBe("Имя вида не может быть пустым");
    expect(particleKindNameError("искры", "дым", names)).toBe("Вид «искры» уже есть");
  });

  it("новое имя и прежнее имя этого же вида годятся", () => {
    expect(particleKindNameError("туман", "дым", names)).toBeUndefined();
    expect(particleKindNameError("дым", "дым", names)).toBeUndefined();
  });
});
