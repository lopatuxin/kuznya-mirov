import { beginAction, beginTerrainCreation, type EditSessionState, type EditSnapshot } from "./editSession";
import { chooseTerrainFilePath, parseProjectFilePaths } from "./projectFiles";
import { parseSceneSize, type SceneSize } from "./sceneObjects";
import { addTerrainFilePath } from "./sceneTextEditing";
import { flatTerrainGrid, formatTerrainText } from "./terrainFile";

/** Что действие с рельефом делает с правкой: новое состояние и `game.json`, в который первое действие дописывает `files.terrain`. */
export type TerrainEditPlan = { state: EditSessionState; gameJsonText: string };

/**
 * Действие с рельефом — «Кисти рельефа», требования 18–19, 23: `build` считает новый текст файла
 * рельефа. У проекта с файлом это обычное действие правки. У проекта без файла первое действие заводит его:
 * имя `terrain.json` (или `terrain-2.json` и дальше, если занято), ключ `files.terrain` дописан в
 * `game.json`, а отмена вернёт ровную землю, не «файла нет». `null` — действия нет: нечего править
 * или текст не изменился.
 */
export async function planTerrainEdit(
  session: EditSessionState,
  gameJsonText: string,
  isFilePresent: (relativePath: string) => Promise<boolean>,
  build: (displayed: EditSnapshot, sceneSize: SceneSize) => string | null,
): Promise<TerrainEditPlan | null> {
  const sceneSize = parseSceneSize(gameJsonText);
  const paths = parseProjectFilePaths(gameJsonText);
  if (sceneSize === null || paths === null) return null;
  const terrainText = build(session.displayed, sceneSize);
  if (terrainText === null || terrainText === session.displayed.terrainText) return null;
  const candidate: EditSnapshot = { ...session.displayed, terrainText };
  if (session.displayed.terrainText !== null) return { state: beginAction(session, candidate), gameJsonText };
  const terrainPath = await chooseTerrainFilePath(paths.scene, isFilePresent);
  const flatText = formatTerrainText({ ...flatTerrainGrid(sceneSize), water: null, covers: null });
  return { state: beginTerrainCreation(session, candidate, flatText), gameJsonText: addTerrainFilePath(gameJsonText, terrainPath) };
}
