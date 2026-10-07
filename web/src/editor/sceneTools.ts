import type { HandleMode } from "./handleGeometry";
import type { BrushKind } from "./terrainBrush";

/**
 * Выбран всегда один инструмент: вид ручек (`brushKind === null`, `isImprintTool` и `isPaintTool` ложны), кисть —
 * «Кисти рельефа», требование 1, «Отпечаток» — «Редактор», «Правка сцены», требование 22, или «Покрасить» — «Покраска», требование 1.
 */
export type SelectedTool = { handleMode: HandleMode; brushKind: BrushKind | null; isImprintTool: boolean; isPaintTool: boolean };

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
 * Ручки и кнопки видов ручек — в обеих сценах, пока сцена показана, правка доступна и партия не идёт (вне партии и
 * на паузе); кисти и их поля — только в трёхмерной сцене и ещё только вне партии, паузы и повтора («Кисти рельефа»,
 * требование 4). При ошибках проекта и без правки сцены нет ни тех, ни других.
 */
export function resolveSceneToolAvailability(context: SceneToolContext): SceneToolAvailability {
  const areHandlesAvailable = context.isSceneShown && context.canEditScene && !context.isGameInputActive;
  return { areHandlesAvailable, areBrushesAvailable: areHandlesAvailable && context.isThreeDimensionalScene && context.isEditorCameraActive };
}

/**
 * Кнопка «Ветер» — «Правка сцены», требование 30: только в плоской сцене; неактивна, когда сцены нет (в проекте ошибки)
 * или её нельзя править (повтор). Ветер правится и в партии, и на паузе, поэтому от ручек и кистей кнопка не зависит.
 */
export function resolveWindMenuState(context: Pick<SceneToolContext, "isThreeDimensionalScene" | "isSceneShown" | "canEditScene">): "hidden" | "disabled" | "enabled" {
  if (context.isThreeDimensionalScene) return "hidden";
  return context.isSceneShown && context.canEditScene ? "enabled" : "disabled";
}

/** Кнопка «Отпечаток» нажимается там же, где кисти, и только когда в игре объявлены штампы («Редактор», «Правка сцены», требование 23). */
export function isImprintToolEnabled(areBrushesAvailable: boolean, hasStamps: boolean): boolean {
  return areBrushesAvailable && hasStamps;
}

/** Группа «Материалы» доступна там же, где кисти, и только когда в игре объявлены материалы («Покраска», требование 4). */
export function isPaintToolEnabled(areBrushesAvailable: boolean, hasMaterials: boolean): boolean {
  return areBrushesAvailable && hasMaterials;
}

/** Виды ручек выбираются вместо кисти, «Отпечатка» и «Покрасить» («Кисти рельефа», требование 1). */
export function selectHandleModeTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null, isImprintTool: false, isPaintTool: false };
}

export function selectBrushTool(handleMode: HandleMode, brushKind: BrushKind): SelectedTool {
  return { handleMode, brushKind, isImprintTool: false, isPaintTool: false };
}

export function selectImprintTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null, isImprintTool: true, isPaintTool: false };
}

export function selectPaintTool(handleMode: HandleMode): SelectedTool {
  return { handleMode, brushKind: null, isImprintTool: false, isPaintTool: true };
}

/**
 * Кисти, «Отпечаток» или «Покрасить» пропали (партия, пауза, повтор, плоская сцена, штампов или материалов нет) —
 * вместо них выбран «Перенос»; иначе выбор остаётся.
 */
export function settleSelectedTool(tool: SelectedTool, areBrushesAvailable: boolean, isImprintEnabled: boolean, isPaintEnabled: boolean): SelectedTool {
  const isBrushLost = tool.brushKind !== null && !areBrushesAvailable;
  const isImprintLost = tool.isImprintTool && !isImprintEnabled;
  const isPaintLost = tool.isPaintTool && !isPaintEnabled;
  return isBrushLost || isImprintLost || isPaintLost ? selectHandleModeTool("translate") : tool;
}
