import { describe, expect, it } from "vitest";
import { resolveSceneToolAvailability, selectHandleModeTool, settleSelectedTool, type SceneToolContext } from "./sceneTools";

const EDITING: SceneToolContext = {
  isThreeDimensionalScene: true,
  isSceneShown: true,
  canEditScene: true,
  isGameInputActive: false,
  isEditorCameraActive: true,
};

describe("resolveSceneToolAvailability", () => {
  it("в трёхмерной сцене вне партии есть и ручки, и кисти", () => {
    expect(resolveSceneToolAvailability(EDITING)).toEqual({ areHandlesAvailable: true, areBrushesAvailable: true });
  });

  it("в плоской сцене нет ни ручек, ни кистей", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isThreeDimensionalScene: false })).toEqual({ areHandlesAvailable: false, areBrushesAvailable: false });
  });

  it("в идущей партии и на паузе нет ни ручек, ни кистей", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isGameInputActive: true, isEditorCameraActive: false })).toEqual({
      areHandlesAvailable: false,
      areBrushesAvailable: false,
    });
  });

  it("в повторе камеру редактора не водят: кистей нет", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isEditorCameraActive: false }).areBrushesAvailable).toBe(false);
  });

  it("при ошибках проекта сцена не показана: ни ручек, ни кистей", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isSceneShown: false })).toEqual({ areHandlesAvailable: false, areBrushesAvailable: false });
  });

  it("сцену нельзя править: ни ручек, ни кистей", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, canEditScene: false })).toEqual({ areHandlesAvailable: false, areBrushesAvailable: false });
  });
});

describe("selectHandleModeTool", () => {
  it("вид ручек снимает кисть", () => {
    expect(selectHandleModeTool("rotate")).toEqual({ handleMode: "rotate", brushKind: null });
  });
});

describe("settleSelectedTool", () => {
  it("«Запуск» при выбранной кисти выбирает «Перенос», а не прежний вид ручек", () => {
    expect(settleSelectedTool({ handleMode: "scale", brushKind: "raise" }, false)).toEqual({ handleMode: "translate", brushKind: null });
  });

  it("кисть остаётся, пока кисти доступны", () => {
    const tool = { handleMode: "scale" as const, brushKind: "smooth" as const };
    expect(settleSelectedTool(tool, true)).toBe(tool);
  });

  it("выбранные ручки остаются, когда кистей нет: после «Стопа» вид ручек тот же", () => {
    const tool = { handleMode: "rotate" as const, brushKind: null };
    expect(settleSelectedTool(tool, false)).toBe(tool);
  });
});
