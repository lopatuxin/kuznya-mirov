import { describe, expect, it } from "vitest";
import {
  applyExternalRead,
  beginAction,
  beginTerrainCreation,
  beginUndo,
  createEditSessionState,
  dirtyFiles,
  isReloadStale,
  markUnsaved,
  markWritten,
  type EditSnapshot,
} from "./editSession";

const INITIAL: EditSnapshot = { sceneText: "scene-0", propertiesText: "props-0", terrainText: null };

describe("beginAction / markWritten", () => {
  it("действие кладёт показанный текст в историю и показывает новый", () => {
    const state = createEditSessionState(INITIAL);
    const next = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null });

    expect(next.displayed).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
    expect(next.history).toEqual([INITIAL]);
  });

  it("запись без ошибок сдвигает diskTruth к показанному и снимает «не сохранено»", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
    state = markWritten(state);

    expect(state.diskTruth).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
    expect(state.saveState).toEqual({ status: "saved" });
  });

  it("ошибка проверки — показанное остаётся, diskTruth не двигается", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
    state = markUnsaved(state, "в проекте ошибки");

    expect(state.displayed.sceneText).toBe("scene-1");
    expect(state.diskTruth).toEqual(INITIAL);
    expect(state.saveState).toEqual({ status: "unsaved", reason: "в проекте ошибки" });
  });
});

describe("dirtyFiles", () => {
  it("отмечает только те файлы, что отличаются от diskTruth", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null });

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

    expect(next.displayed).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: "terrain-pending" });
  });
});

describe("beginUndo", () => {
  it("истории нет — null, кнопка неактивна", () => {
    const state = createEditSessionState(INITIAL);
    expect(beginUndo(state)).toBe(null);
  });

  it("возвращает последний снимок и снимает его с истории, без записи повтора", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
    state = markWritten(state);

    const undo = beginUndo(state);
    expect(undo).not.toBe(null);
    expect(undo?.candidate).toEqual(INITIAL);
    expect(undo?.state.displayed).toEqual(INITIAL);
    expect(undo?.state.history).toEqual([]);
  });

  it("многошаговая отмена — второй Ctrl+Z откатывает предыдущий шаг", () => {
    let state = createEditSessionState(INITIAL);
    state = markWritten(beginAction(state, { sceneText: "scene-1", propertiesText: "props-0", terrainText: null }));
    state = markWritten(beginAction(state, { sceneText: "scene-2", propertiesText: "props-0", terrainText: null }));

    const firstUndo = beginUndo(state);
    expect(firstUndo?.candidate).toEqual({ sceneText: "scene-1", propertiesText: "props-0", terrainText: null });
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
    const next = applyExternalRead(state, { sceneText: "scene-external", propertiesText: "props-0", terrainText: null });

    expect(next.history).toEqual([INITIAL]);
    expect(next.displayed).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: null });
    expect(next.diskTruth).toEqual({ sceneText: "scene-external", propertiesText: "props-0", terrainText: null });
  });

  it("правка other-файла не трогает несохранённую правку в displayed — требование 23 (переоценка снаружи хука)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0", terrainText: null });
    // Внешняя правка не пришла (diskTruth не изменился) — applyExternalRead её и не находит.
    const next = applyExternalRead(state, INITIAL);
    expect(next).toBe(state);
    expect(next.displayed.sceneText).toBe("scene-pending");
  });

  it("правка properties.json при несохранённой правке scene.json — уходит в историю то, что было показано (включая несохранённый scene)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0", terrainText: null });

    const next = applyExternalRead(state, { sceneText: "scene-0", propertiesText: "props-external", terrainText: null });

    expect(next.history.at(-1)).toEqual({ sceneText: "scene-pending", propertiesText: "props-0", terrainText: null });
    // scene.json на диске не изменился — несохранённая правка scene остаётся на экране.
    expect(next.displayed).toEqual({ sceneText: "scene-pending", propertiesText: "props-external", terrainText: null });
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
