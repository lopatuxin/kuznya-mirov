import { describe, expect, it } from "vitest";
import {
  isMountainToolEnabled,
  resolveSceneToolAvailability,
  selectBrushTool,
  selectHandleModeTool,
  selectMountainTool,
  settleSelectedTool,
  type SceneToolContext,
} from "./sceneTools";

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

describe("isMountainToolEnabled", () => {
  it("кнопка «Гора» нажимается там, где есть кисти, и только со штампами", () => {
    expect(isMountainToolEnabled(true, true)).toBe(true);
    expect(isMountainToolEnabled(true, false)).toBe(false);
  });

  it("в партии, на паузе, в повторе и в плоской сцене кистей нет — «Горы» тоже", () => {
    expect(isMountainToolEnabled(false, true)).toBe(false);
  });
});

describe("выбор инструмента", () => {
  it("вид ручек снимает кисть и «Гору»", () => {
    expect(selectHandleModeTool("rotate")).toEqual({ handleMode: "rotate", brushKind: null, isMountainTool: false });
  });

  it("кисть и «Гора» заменяют друг друга", () => {
    expect(selectBrushTool("scale", "level")).toEqual({ handleMode: "scale", brushKind: "level", isMountainTool: false });
    expect(selectMountainTool("scale")).toEqual({ handleMode: "scale", brushKind: null, isMountainTool: true });
  });
});

describe("settleSelectedTool", () => {
  it("«Запуск» при выбранной кисти выбирает «Перенос», а не прежний вид ручек", () => {
    expect(settleSelectedTool(selectBrushTool("scale", "raise"), false, false)).toEqual(selectHandleModeTool("translate"));
  });

  it("кисть остаётся, пока кисти доступны", () => {
    const tool = selectBrushTool("scale", "smooth");
    expect(settleSelectedTool(tool, true, true)).toBe(tool);
  });

  it("выбранные ручки остаются, когда кистей нет: после «Стопа» вид ручек тот же", () => {
    const tool = selectHandleModeTool("rotate");
    expect(settleSelectedTool(tool, false, false)).toBe(tool);
  });

  it("«Гора» уходит в «Перенос», когда пропали кисти или штампы, и остаётся, пока доступна", () => {
    const tool = selectMountainTool("rotate");
    expect(settleSelectedTool(tool, true, true)).toBe(tool);
    expect(settleSelectedTool(tool, true, false)).toEqual(selectHandleModeTool("translate"));
    expect(settleSelectedTool(tool, false, false)).toEqual(selectHandleModeTool("translate"));
  });
});
