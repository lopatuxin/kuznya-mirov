import { describe, expect, it } from "vitest";
import {
  applyExternalRead,
  beginAction,
  beginTerrainCreation,
  beginUndo,
  createEditSessionState,
  dirtyFiles,
  dirtyMaskPaths,
  isReloadStale,
  markUnsaved,
  markWritten,
  type EditSnapshot,
} from "./editSession";
import { NO_MASKS } from "./maskBytes";

const INITIAL: EditSnapshot = { sceneText: "scene-0", propertiesText: "props-0", terrainText: null, masks: NO_MASKS };

describe("beginAction / markWritten", () => {
  it("действие кладёт показанный текст в историю и показывает новый", () => {
    const state = createEditSessionState(INITIAL);
    const next = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });

    expect(next.displayed).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    expect(next.history).toEqual([INITIAL]);
  });

  it("запись без ошибок сдвигает diskTruth к показанному и снимает «не сохранено»", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    state = markWritten(state);

    expect(state.diskTruth).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    expect(state.saveState).toEqual({ status: "saved" });
  });

  it("ошибка проверки — показанное остаётся, diskTruth не двигается", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    state = markUnsaved(state, "в проекте ошибки");

    expect(state.displayed.sceneText).toBe("scene-1");
    expect(state.diskTruth).toEqual(INITIAL);
    expect(state.saveState).toEqual({ status: "unsaved", reason: "в проекте ошибки" });
  });
});

describe("dirtyFiles", () => {
  it("отмечает только те файлы, что отличаются от diskTruth", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });

    expect(dirtyFiles(state)).toEqual({ scene: true, properties: false, terrain: false });
  });
});

describe("рельеф в снимке", () => {
  const WITH_TERRAIN: EditSnapshot = { ...INITIAL, terrainText: "terrain-0" };

  it("рельеф, что отличается от diskTruth, помечен к записи", () => {
    const state = beginAction(createEditSessionState(WITH_TERRAIN), { ...WITH_TERRAIN, terrainText: "terrain-1" });

    expect(dirtyFiles(state)).toEqual({ scene: false, properties: false, terrain: true });
  });

  it("первое изменение рельефа без файла: снимок «до» — ровная земля, а не «файла нет», и отмена его возвращает", () => {
    const state = beginTerrainCreation(createEditSessionState(INITIAL), { ...INITIAL, terrainText: "terrain-1" }, "flat");

    expect(state.history).toEqual([{ ...INITIAL, terrainText: "flat" }]);
    expect(state.displayed.terrainText).toBe("terrain-1");
    expect(dirtyFiles(state).terrain).toBe(true);
    expect(beginUndo(state)?.candidate.terrainText).toBe("flat");
  });

  it("рельеф изменился снаружи — в историю уходит показанное до, экран получает свежий текст, Ctrl+Z вернёт прежний", () => {
    const state = createEditSessionState(WITH_TERRAIN);
    const next = applyExternalRead(state, { ...WITH_TERRAIN, terrainText: "terrain-external" });

    expect(next.displayed.terrainText).toBe("terrain-external");
    expect(beginUndo(next)?.candidate.terrainText).toBe("terrain-0");
  });

  it("правка scene.json снаружи не трогает несохранённый рельеф", () => {
    const pending = beginAction(createEditSessionState(WITH_TERRAIN), { ...WITH_TERRAIN, terrainText: "terrain-pending" });
    const next = applyExternalRead(pending, { ...WITH_TERRAIN, sceneText: "scene-external" });

    expect(next.displayed).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: "terrain-pending", masks: NO_MASKS });
  });
});

describe("beginUndo", () => {
  it("истории нет — null, кнопка неактивна", () => {
    const state = createEditSessionState(INITIAL);
    expect(beginUndo(state)).toBe(null);
  });

  it("возвращает последний снимок и снимает его с истории, без записи повтора", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    state = markWritten(state);

    const undo = beginUndo(state);
    expect(undo).not.toBe(null);
    expect(undo?.candidate).toEqual(INITIAL);
    expect(undo?.state.displayed).toEqual(INITIAL);
    expect(undo?.state.history).toEqual([]);
  });

  it("многошаговая отмена — второй Ctrl+Z откатывает предыдущий шаг", () => {
    let state = createEditSessionState(INITIAL);
    state = markWritten(beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS }));
    state = markWritten(beginAction(state, { sceneText: "scene-2", propertiesText: "props-0", terrainText: null, masks: NO_MASKS }));

    const firstUndo = beginUndo(state);
    expect(firstUndo?.candidate).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    state = markWritten(firstUndo?.state as typeof state);

    const secondUndo = beginUndo(state);
    expect(secondUndo?.candidate).toEqual(INITIAL);
  });
});

describe("applyExternalRead", () => {
  it("прочитанное совпадает с diskTruth — состояние не меняется", () => {
    const state = createEditSessionState(INITIAL);
    const next = applyExternalRead(state, INITIAL);
    expect(next).toBe(state);
  });

  it("scene.json изменился снаружи — в историю уходит показанное до, экран получает свежий текст", () => {
    const state = createEditSessionState(INITIAL);
    const next = applyExternalRead(state, { sceneText: "scene-external", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });

    expect(next.history).toEqual([INITIAL]);
    expect(next.displayed).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    expect(next.diskTruth).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
  });

  it("правка other-файла не трогает несохранённую правку в displayed — требование 23 (переоценка снаружи хука)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    // Внешняя правка не пришла (diskTruth не изменился) — applyExternalRead её и не находит.
    const next = applyExternalRead(state, INITIAL);
    expect(next).toBe(state);
    expect(next.displayed.sceneText).toBe("scene-pending");
  });

  it("правка properties.json при несохранённой правке scene.json — уходит в историю то, что было показано (включая несохранённый scene)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });

    const next = applyExternalRead(state, { sceneText: "scene-0", propertiesText: "props-external", terrainText: null, masks: NO_MASKS });

    expect(next.history.at(-1)).toEqual({ sceneText: "scene-pending", propertiesText: "props-0", terrainText: null, masks: NO_MASKS });
    // scene.json на диске не изменился — несохранённая правка scene остаётся на экране.
    expect(next.displayed).toEqual({ sceneText: "scene-pending", propertiesText: "props-external", terrainText: null, masks: NO_MASKS });
  });
});

describe("isReloadStale", () => {
  it("запись случилась после того, как перезагрузка начала читать, — устарело", () => {
    expect(isReloadStale(1, 2)).toBe(true);
  });

  it("записи не было с начала чтения — не устарело", () => {
    expect(isReloadStale(1, 1)).toBe(false);
  });
});

describe("маски покрытий в снимке («Покраска», требование 16)", () => {
  const ROCK = { width: 2, height: 1, pixels: Uint8Array.of(0, 0) };
  const ROCK_PAINTED = { width: 2, height: 1, pixels: Uint8Array.of(90, 0) };
  const WITH_ROCK: EditSnapshot = { ...INITIAL, terrainText: "terrain-0", masks: { "terrain/rock.png": ROCK } };

  it("действие кладёт в историю прежние байты масок; не записанная маска — в dirtyMaskPaths, записанная — нет", () => {
    const state = beginAction(createEditSessionState(WITH_ROCK), { ...WITH_ROCK, masks: { "terrain/rock.png": ROCK_PAINTED } });

    expect(state.history[0]?.masks["terrain/rock.png"]).toBe(ROCK);
    expect(dirtyMaskPaths(state)).toEqual(["terrain/rock.png"]);
    expect(dirtyMaskPaths(markWritten(state))).toEqual([]);
  });

  it("отмена возвращает байты масок до мазка, и они снова не записаны", () => {
    const written = markWritten(beginAction(createEditSessionState(WITH_ROCK), { ...WITH_ROCK, masks: { "terrain/rock.png": ROCK_PAINTED } }));

    const undo = beginUndo(written);

    expect(undo?.candidate.masks["terrain/rock.png"]).toBe(ROCK);
    expect(dirtyMaskPaths(undo?.state as typeof written)).toEqual(["terrain/rock.png"]);
  });

  it("отмена мазка, что положил новый слой, убирает слой: маски нового пути в снимке нет, писать нечего", () => {
    const painted: EditSnapshot = { ...WITH_ROCK, terrainText: "terrain-with-layer", masks: { ...WITH_ROCK.masks, "terrain/scree.png": ROCK_PAINTED } };
    const written = markWritten(beginAction(createEditSessionState(WITH_ROCK), painted));

    const undo = beginUndo(written);

    expect(undo?.candidate.terrainText).toBe("terrain-0");
    expect(Object.keys(undo?.candidate.masks ?? {})).toEqual(["terrain/rock.png"]);
    expect(dirtyMaskPaths(undo?.state as typeof written)).toEqual([]);
    expect(dirtyFiles(undo?.state as typeof written).terrain).toBe(true);
  });

  it("маска изменилась снаружи (команда мазков, другая программа) — в историю уходят прежние байты, Ctrl+Z их вернёт", () => {
    const state = createEditSessionState(WITH_ROCK);

    const next = applyExternalRead(state, { ...WITH_ROCK, masks: { "terrain/rock.png": ROCK_PAINTED } });

    expect(next.displayed.masks["terrain/rock.png"]).toBe(ROCK_PAINTED);
    expect(next.diskTruth.masks["terrain/rock.png"]).toBe(ROCK_PAINTED);
    expect(dirtyMaskPaths(next)).toEqual([]);
    expect(beginUndo(next)?.candidate.masks["terrain/rock.png"]).toBe(ROCK);
  });

  it("байты те же в других массивах — не внешняя правка: состояние не меняется", () => {
    const state = createEditSessionState(WITH_ROCK);

    const next = applyExternalRead(state, { ...WITH_ROCK, masks: { "terrain/rock.png": { width: 2, height: 1, pixels: Uint8Array.of(0, 0) } } });

    expect(next).toBe(state);
  });

  it("правка файла снаружи при несохранённой маске не затирает её", () => {
    const pending = beginAction(createEditSessionState(WITH_ROCK), { ...WITH_ROCK, masks: { "terrain/rock.png": ROCK_PAINTED } });

    const next = applyExternalRead(pending, { ...WITH_ROCK, sceneText: "scene-external" });

    expect(next.displayed.masks["terrain/rock.png"]).toBe(ROCK_PAINTED);
    expect(next.displayed.sceneText).toBe("scene-external");
  });
});
