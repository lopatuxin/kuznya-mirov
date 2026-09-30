import type { HandleMode } from "./handleGeometry";
import type { BrushKind } from "./terrainBrush";

/** Выбран всегда один инструмент: вид ручек (`brushKind === null`) или кисть — «Кисти рельефа», требование 1. */
export type SelectedTool = { handleMode: HandleMode; brushKind: BrushKind | null };

export type SceneToolContext = {
  isThreeDimensionalScene: boolean;
  isSceneShown: boolean;
  canEditScene: boolean;
  /** Партия идёт или стоит на паузе. */
  isGameInputActive: boolean;
  /** Камеру водит редактор: вне партии, паузы и повтора. */
  isEditorCameraActive: boolean;
};

export type SceneToolAvailability = { areHandlesAvailable: boolean; areBrushesAvailable: boolean };

/**
 * Ручки и кнопки видов ручек — только в трёхмерной сцене; кисти и их поля — ещё и только вне партии,
 * паузы и повтора («Кисти рельефа», требование 4): в плоской сцене, при ошибках проекта и без правки
 * сцены нет ни тех, ни других.
 */
export function resolveSceneToolAvailability(context: SceneToolContext): SceneToolAvailability {
  const areHandlesAvailable = context.isThreeDimensionalScene && context.isSceneShown && context.canEditScene && !context.isGameInputActive;
  return { areHandlesAvailable, areBrushesAvailable: areHandlesAvailable && context.isEditorCameraActive };
}

/** Виды ручек выбираются вместо кисти («Кисти рельефа», требование 1). */
export function selectHandleModeTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null };
}

/** Кисти пропали (партия, пауза, повтор, плоская сцена) — вместо кисти выбран «Перенос»; иначе выбор остаётся. */
export function settleSelectedTool(tool: SelectedTool, areBrushesAvailable: boolean): SelectedTool {
  if (areBrushesAvailable || tool.brushKind === null) return tool;
  return selectHandleModeTool("translate");
}
