import { beginAction, beginMotionsCreation, type EditSessionState, type EditSnapshot } from "./editSession";
import { EMPTY_MOTIONS_TEXT, motionsTextWithNewMotion } from "./motionsFile";
import { chooseMotionsFilePath, parseProjectFilePaths } from "./projectFiles";
import { addMotionsFilePath } from "./sceneTextEditing";

/** Что новое движение делает с правкой: новое состояние и `game.json`, в который первое движение дописывает `files.motions`. */
export type MotionCreationPlan = { state: EditSessionState; gameJsonText: string };

/**
 * Новое движение — «Анимация», требование 5: дописывается в `motions.json`. У игры без файла движений первое движение заводит
 * его: имя `motions.json` (или `motions-2.json` и дальше, если занято), ключ `files.motions` дописан в `game.json`, а отмена вернёт
 * пустую таблицу, не «файла нет». `null` — действия нет: имя пустое или занято.
 */
export async function planMotionCreation(
  session: EditSessionState,
  gameJsonText: string,
  isFilePresent: (relativePath: string) => Promise<boolean>,
  motion: { name: string; model: string; clip: string },
): Promise<MotionCreationPlan | null> {
  const paths = parseProjectFilePaths(gameJsonText);
  if (paths === null) return null;
  const motionsText = motionsTextWithNewMotion(session.displayed.motionsText, motion.name, motion.model, motion.clip);
  if (motionsText === null) return null;
  const candidate: EditSnapshot = { ...session.displayed, motionsText };
  if (session.displayed.motionsText !== null) return { state: beginAction(session, candidate), gameJsonText };
  const state = beginMotionsCreation(session, candidate, EMPTY_MOTIONS_TEXT);
  if (paths.motions !== null) return { state, gameJsonText };
  return { state, gameJsonText: addMotionsFilePath(gameJsonText, await chooseMotionsFilePath(isFilePresent)) };
}
