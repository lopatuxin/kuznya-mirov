import { describe, expect, it } from "vitest";
import {
  isImprintToolEnabled,
  isPaintToolEnabled,
  resolveSceneToolAvailability,
  resolveWindMenuState,
  selectBrushTool,
  selectHandleModeTool,
  selectImprintTool,
  selectPaintTool,
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

  it("в плоской сцене вне партии ручки есть, кистей нет", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isThreeDimensionalScene: false })).toEqual({ areHandlesAvailable: true, areBrushesAvailable: false });
  });

  it("в плоской сцене на паузе ручки есть, в идущей партии нет", () => {
    expect(resolveSceneToolAvailability({ ...EDITING, isThreeDimensionalScene: false, isEditorCameraActive: false })).toEqual({ areHandlesAvailable: true, areBrushesAvailable: false });
    expect(resolveSceneToolAvailability({ ...EDITING, isThreeDimensionalScene: false, isGameInputActive: true })).toEqual({ areHandlesAvailable: false, areBrushesAvailable: false });
  });

  it("в идущей партии нет ни ручек, ни кистей", () => {
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

describe("isImprintToolEnabled", () => {
  it("кнопка «Отпечаток» нажимается там, где есть кисти, и только со штампами", () => {
    expect(isImprintToolEnabled(true, true)).toBe(true);
    expect(isImprintToolEnabled(true, false)).toBe(false);
  });

  it("в партии, на паузе, в повторе и в плоской сцене кистей нет и кнопки «Отпечаток»", () => {
    expect(isImprintToolEnabled(false, true)).toBe(false);
  });
});

describe("isPaintToolEnabled", () => {
  it("группа «Материалы» доступна там, где есть кисти, и только с материалами", () => {
    expect(isPaintToolEnabled(true, true)).toBe(true);
    expect(isPaintToolEnabled(true, false)).toBe(false);
  });

  it("в партии, на паузе, в повторе и в плоской сцене кистей нет — «Покрасить» тоже", () => {
    expect(isPaintToolEnabled(false, true)).toBe(false);
  });
});

describe("выбор инструмента", () => {
  it("вид ручек снимает кисть, «Отпечаток» и «Покрасить»", () => {
    expect(selectHandleModeTool("rotate")).toEqual({ handleMode: "rotate", brushKind: null, isImprintTool: false, isPaintTool: false });
  });

  it("кисть, «Отпечаток» и «Покрасить» заменяют друг друга", () => {
    expect(selectBrushTool("scale", "level")).toEqual({ handleMode: "scale", brushKind: "level", isImprintTool: false, isPaintTool: false });
    expect(selectImprintTool("scale")).toEqual({ handleMode: "scale", brushKind: null, isImprintTool: true, isPaintTool: false });
    expect(selectPaintTool("scale")).toEqual({ handleMode: "scale", brushKind: null, isImprintTool: false, isPaintTool: true });
  });
});

describe("settleSelectedTool", () => {
  it("«Запуск» при выбранной кисти выбирает «Перенос», а не прежний вид ручек", () => {
    expect(settleSelectedTool(selectBrushTool("scale", "raise"), false, false, false)).toEqual(selectHandleModeTool("translate"));
  });

  it("кисть остаётся, пока кисти доступны", () => {
    const tool = selectBrushTool("scale", "smooth");
    expect(settleSelectedTool(tool, true, true, true)).toBe(tool);
  });

  it("выбранные ручки остаются, когда кистей нет: после «Стопа» вид ручек тот же", () => {
    const tool = selectHandleModeTool("rotate");
    expect(settleSelectedTool(tool, false, false, false)).toBe(tool);
  });

  it("«Отпечаток» уходит в «Перенос», когда пропали кисти или штампы, и остаётся, пока доступна", () => {
    const tool = selectImprintTool("rotate");
    expect(settleSelectedTool(tool, true, true, true)).toBe(tool);
    expect(settleSelectedTool(tool, true, false, true)).toEqual(selectHandleModeTool("translate"));
    expect(settleSelectedTool(tool, false, false, false)).toEqual(selectHandleModeTool("translate"));
  });
});

describe("settleSelectedTool — «Покрасить»", () => {
  it("уходит в «Перенос», когда пропали кисти или материалы, и остаётся, пока доступна", () => {
    const tool = selectPaintTool("rotate");
    expect(settleSelectedTool(tool, true, true, true)).toBe(tool);
    expect(settleSelectedTool(tool, true, true, false)).toEqual(selectHandleModeTool("translate"));
    expect(settleSelectedTool(tool, false, false, false)).toEqual(selectHandleModeTool("translate"));
  });
});

describe("resolveWindMenuState", () => {
  const FLAT = { isThreeDimensionalScene: false, isSceneShown: true, canEditScene: true };

  it("в плоской сцене кнопка есть и активна — и вне партии, и в идущей партии: ручки на ней не отражаются", () => {
    expect(resolveWindMenuState(FLAT)).toBe("enabled");
  });

  it("в трёхмерной сцене кнопки нет", () => {
    expect(resolveWindMenuState({ ...FLAT, isThreeDimensionalScene: true })).toBe("hidden");
  });

  it("при ошибках проекта кнопка неактивна", () => {
    expect(resolveWindMenuState({ ...FLAT, isSceneShown: false })).toBe("disabled");
  });

  it("в повторе, где сцену править нельзя, кнопка неактивна", () => {
    expect(resolveWindMenuState({ ...FLAT, canEditScene: false })).toBe("disabled");
  });
});
