import type { HandleMode } from "./handleGeometry";
import type { BrushKind } from "./terrainBrush";

/**
 * Выбран всегда один инструмент: вид ручек (`brushKind === null` и `isMountainTool` ложно), кисть —
 * «Кисти рельефа», требование 1, или «Гора» — «Редактор», «Правка сцены», требование 22.
 */
export type SelectedTool = { handleMode: HandleMode; brushKind: BrushKind | null; isMountainTool: boolean };

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

/** Кнопка «Гора» нажимается там же, где кисти, и только когда в игре объявлены штампы («Редактор», «Правка сцены», требование 23). */
export function isMountainToolEnabled(areBrushesAvailable: boolean, hasStamps: boolean): boolean {
  return areBrushesAvailable && hasStamps;
}

/** Виды ручек выбираются вместо кисти и «Горы» («Кисти рельефа», требование 1). */
export function selectHandleModeTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null, isMountainTool: false };
}

export function selectBrushTool(handleMode: HandleMode, brushKind: BrushKind): SelectedTool {
  return { handleMode, brushKind, isMountainTool: false };
}

export function selectMountainTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null, isMountainTool: true };
}

/**
 * Кисти или «Гора» пропали (партия, пауза, повтор, плоская сцена, штампов нет) — вместо них выбран
 * «Перенос»; иначе выбор остаётся.
 */
export function settleSelectedTool(tool: SelectedTool, areBrushesAvailable: boolean, isMountainEnabled: boolean): SelectedTool {
  const isBrushLost = tool.brushKind !== null && !areBrushesAvailable;
  const isMountainLost = tool.isMountainTool && !isMountainEnabled;
  return isBrushLost || isMountainLost ? selectHandleModeTool("translate") : tool;
}
