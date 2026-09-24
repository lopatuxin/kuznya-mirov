import { describe, expect, it } from "vitest";
import {
  applyExternalRead,
  beginAction,
  beginUndo,
  createEditSessionState,
  dirtyFiles,
  isReloadStale,
  markUnsaved,
  markWritten,
  type EditSnapshot,
} from "./editSession";

const INITIAL: EditSnapshot = { sceneText: "scene-0", propertiesText: "props-0" };

describe("beginAction / markWritten", () => {
  it("действие кладёт показанный текст в историю и показывает новый", () => {
    const state = createEditSessionState(INITIAL);
    const next = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" });

    expect(next.displayed).toEqual({ sceneText: "scene-1", propertiesText: "props-0" });
    expect(next.history).toEqual([INITIAL]);
  });

  it("запись без ошибок сдвигает diskTruth к показанному и снимает «не сохранено»", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" });
    state = markWritten(state);

    expect(state.diskTruth).toEqual({ sceneText: "scene-1", propertiesText: "props-0" });
    expect(state.saveState).toEqual({ status: "saved" });
  });

  it("ошибка проверки — показанное остаётся, diskTruth не двигается", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" });
    state = markUnsaved(state, "в проекте ошибки");

    expect(state.displayed.sceneText).toBe("scene-1");
    expect(state.diskTruth).toEqual(INITIAL);
    expect(state.saveState).toEqual({ status: "unsaved", reason: "в проекте ошибки" });
  });
});

describe("dirtyFiles", () => {
  it("отмечает только те файлы, что отличаются от diskTruth", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" });

    expect(dirtyFiles(state)).toEqual({ scene: true, properties: false });
  });
});

describe("beginUndo", () => {
  it("истории нет — null, кнопка неактивна", () => {
    const state = createEditSessionState(INITIAL);
    expect(beginUndo(state)).toBe(null);
  });

  it("возвращает последний снимок и снимает его с истории, без записи повтора", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" });
    state = markWritten(state);

    const undo = beginUndo(state);
    expect(undo).not.toBe(null);
    expect(undo?.candidate).toEqual(INITIAL);
    expect(undo?.state.displayed).toEqual(INITIAL);
    expect(undo?.state.history).toEqual([]);
  });

  it("многошаговая отмена — второй Ctrl+Z откатывает предыдущий шаг", () => {
    let state = createEditSessionState(INITIAL);
    state = markWritten(beginAction(state, { sceneText: "scene-1", propertiesText: "props-0" }));
    state = markWritten(beginAction(state, { sceneText: "scene-2", propertiesText: "props-0" }));

    const firstUndo = beginUndo(state);
    expect(firstUndo?.candidate).toEqual({ sceneText: "scene-1", propertiesText: "props-0" });
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
    const next = applyExternalRead(state, { sceneText: "scene-external", propertiesText: "props-0" });

    expect(next.history).toEqual([INITIAL]);
    expect(next.displayed).toEqual({ sceneText: "scene-external", propertiesText: "props-0" });
    expect(next.diskTruth).toEqual({ sceneText: "scene-external", propertiesText: "props-0" });
  });

  it("правка other-файла не трогает несохранённую правку в displayed — требование 23 (переоценка снаружи хука)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0" });
    // Внешняя правка не пришла (diskTruth не изменился) — applyExternalRead её и не находит.
    const next = applyExternalRead(state, INITIAL);
    expect(next).toBe(state);
    expect(next.displayed.sceneText).toBe("scene-pending");
  });

  it("правка properties.json при несохранённой правке scene.json — уходит в историю то, что было показано (включая несохранённый scene)", () => {
    let state = createEditSessionState(INITIAL);
    state = beginAction(state, { sceneText: "scene-pending", propertiesText: "props-0" });

    const next = applyExternalRead(state, { sceneText: "scene-0", propertiesText: "props-external" });

    expect(next.history.at(-1)).toEqual({ sceneText: "scene-pending", propertiesText: "props-0" });
    // scene.json на диске не изменился — несохранённая правка scene остаётся на экране.
    expect(next.displayed).toEqual({ sceneText: "scene-pending", propertiesText: "props-external" });
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
