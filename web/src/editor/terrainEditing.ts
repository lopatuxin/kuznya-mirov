import { beginAction, beginTerrainCreation, type EditSessionState, type EditSnapshot } from "./editSession";
import { NO_MASKS, type MaskSet } from "./maskBytes";
import { chooseTerrainFilePath, parseProjectFilePaths } from "./projectFiles";
import { parseSceneSize, type SceneSize } from "./sceneObjects";
import { addTerrainFilePath } from "./sceneTextEditing";
import { areCoversEqual } from "./paintLayers";
import { flatTerrainGrid, formatTerrainText, readTerrainCovers, terrainTextWithCovers, type TerrainCoverLayer } from "./terrainFile";

/**
 * Текст рельефа после мазка покраски: слои те же — файл остаётся байт в байт («Покраска», требование 12), пишется только
 * маска; слои другие — текст со слоями, а высоты, вода, горы и карта цвета как были.
 */
export function terrainTextAfterPaint(previousText: string | null, sceneSize: SceneSize, covers: readonly TerrainCoverLayer[]): string | null {
  if (previousText !== null && areCoversEqual(readTerrainCovers(previousText), covers)) return previousText;
  return terrainTextWithCovers(previousText, sceneSize, covers);
}

/** Что действие с рельефом делает с правкой: новое состояние и `game.json`, в который первое действие дописывает `files.terrain`. */
export type TerrainEditPlan = { state: EditSessionState; gameJsonText: string };

/**
 * Действие с рельефом — «Кисти рельефа», требования 18–19, 23: `build` считает новый текст файла
 * рельефа. У проекта с файлом это обычное действие правки. У проекта без файла первое действие заводит его:
 * имя `terrain.json` (или `terrain-2.json` и дальше, если занято), ключ `files.terrain` дописан в
 * `game.json`, а отмена вернёт ровную землю, не «файла нет». `null` — действия нет: нечего править
 * или текст не изменился. `maskChanges` — маски, что действие кладёт поверх масок правки («Покраска»): только
 * они изменились — тоже действие.
 */
export async function planTerrainEdit(
  session: EditSessionState,
  gameJsonText: string,
  isFilePresent: (relativePath: string) => Promise<boolean>,
  build: (displayed: EditSnapshot, sceneSize: SceneSize) => string | null,
  maskChanges: MaskSet = NO_MASKS,
): Promise<TerrainEditPlan | null> {
  const sceneSize = parseSceneSize(gameJsonText);
  const paths = parseProjectFilePaths(gameJsonText);
  if (sceneSize === null || paths === null) return null;
  const terrainText = build(session.displayed, sceneSize);
  const hasMaskChanges = Object.keys(maskChanges).length > 0;
  if (terrainText === null || (terrainText === session.displayed.terrainText && !hasMaskChanges)) return null;
  const candidate: EditSnapshot = { ...session.displayed, terrainText, masks: hasMaskChanges ? { ...session.displayed.masks, ...maskChanges } : session.displayed.masks };
  if (session.displayed.terrainText !== null) return { state: beginAction(session, candidate), gameJsonText };
  const terrainPath = await chooseTerrainFilePath(paths.scene, isFilePresent);
  const flatText = formatTerrainText({ ...flatTerrainGrid(sceneSize), water: null, covers: null });
  return { state: beginTerrainCreation(session, candidate, flatText), gameJsonText: addTerrainFilePath(gameJsonText, terrainPath) };
}
