import { areMaskSetsEqual, changedMaskPaths, type MaskSet } from "./maskBytes";

/**
 * Текст файлов, которые правит редактор, и маски покрытий; `terrainText` — `null`, пока у проекта нет файла рельефа
 * («Кисти рельефа», требование 19), `particlesText` — `null`, пока нет файла видов частиц («Ветер и частицы»). `masks` — маски слоёв из `covers` файла рельефа по путям, байты которых история
 * хранит рядом с текстами («Покраска», требование 16).
 */
export type EditSnapshot = { sceneText: string; propertiesText: string; terrainText: string | null; particlesText: string | null; masks: MaskSet };
export type SaveState = { status: "saved" } | { status: "unsaved"; reason: string };

export type EditSessionState = {
  /** Текст, который сейчас показан — может быть несохранённым (требование 2). */
  displayed: EditSnapshot;
  /** Последний текст, про который редактор точно знает, что он на диске — сам прочитал или сам записал. */
  diskTruth: EditSnapshot;
  /** Снимки, показанные перед каждым изменением — требование 20. Живёт, пока открыт проект. */
  history: EditSnapshot[];
  saveState: SaveState;
};

export function createEditSessionState(initial: EditSnapshot): EditSessionState {
  return { displayed: initial, diskTruth: initial, history: [], saveState: { status: "saved" } };
}

/** Действие — требование 20: снимок «до» уходит в историю первым, дальше показан новый текст. */
export function beginAction(state: EditSessionState, candidate: EditSnapshot): EditSessionState {
  return { ...state, history: [...state.history, state.displayed], displayed: candidate };
}

/**
 * Первое изменение рельефа в проекте без файла рельефа — «Кисти рельефа», требование 19: файл
 * заводится вместе с действием и остаётся после отмены, поэтому снимок «до» получает ровную землю
 * (`emptyTerrainText`) вместо «файла нет».
 */
export function beginTerrainCreation(state: EditSessionState, candidate: EditSnapshot, emptyTerrainText: string): EditSessionState {
  return beginAction({ ...state, displayed: { ...state.displayed, terrainText: emptyTerrainText } }, candidate);
}

/**
 * Первый вид частиц в проекте без файла видов — «Редактор», «Правка сцены»: файл заводится вместе с действием и остаётся
 * после отмены пустой таблицей, как файл рельефа, — снимок «до» получает `emptyParticlesText` вместо «файла нет».
 */
export function beginParticlesCreation(state: EditSessionState, candidate: EditSnapshot, emptyParticlesText: string): EditSessionState {
  return beginAction({ ...state, displayed: { ...state.displayed, particlesText: emptyParticlesText } }, candidate);
}

/**
 * Отмена — требование 21: последний снимок истории становится показанным текстом. Без своей
 * записи в историю — повтора отменённого нет (вне scope фазы). Истории нет — `null`, кнопка
 * «Отменить» неактивна.
 */
export function beginUndo(state: EditSessionState): { state: EditSessionState; candidate: EditSnapshot } | null {
  if (state.history.length === 0) return null;
  const candidate = state.history[state.history.length - 1] as EditSnapshot;
  return { state: { ...state, history: state.history.slice(0, -1), displayed: candidate }, candidate };
}

/** Проверка прошла без ошибок и изменившиеся файлы записаны — требования 1–2. */
export function markWritten(state: EditSessionState): EditSessionState {
  return { ...state, diskTruth: state.displayed, saveState: { status: "saved" } };
}

/** Проверка нашла ошибки или отказала запись — правка остаётся на экране, `diskTruth` не двигается. */
export function markUnsaved(state: EditSessionState, reason: string): EditSessionState {
  return { ...state, saveState: { status: "unsaved", reason } };
}

/**
 * Какие файлы отличаются от `diskTruth` и поэтому должны быть записаны — требование 2: следующее
 * успешное действие или отмена дописывают то, что не записалось раньше, вместе со своей правкой.
 */
export function dirtyFiles(state: EditSessionState): { scene: boolean; properties: boolean; terrain: boolean; particles: boolean } {
  return {
    scene: state.displayed.sceneText !== state.diskTruth.sceneText,
    properties: state.displayed.propertiesText !== state.diskTruth.propertiesText,
    terrain: state.displayed.terrainText !== state.diskTruth.terrainText,
    particles: state.displayed.particlesText !== state.diskTruth.particlesText,
  };
}

/** Какие маски отличаются от `diskTruth` и поэтому должны быть записаны — как `dirtyFiles`, но по путям файлов масок. */
export function dirtyMaskPaths(state: EditSessionState): string[] {
  return changedMaskPaths(state.diskTruth.masks, state.displayed.masks);
}

/**
 * Перезагрузка прочитала файлы с диска — требования 22–24. Прочитанное совпадает с `diskTruth` по
 * всем файлам — не внешняя правка, состояние не меняется (несохранённая правка, если она есть,
 * остаётся на экране; вызывающая сторона проверяет её заново). Отличается хотя бы один файл —
 * внешняя правка: в историю уходит показанное до неё, экран получает свежий текст по каждому
 * отличившемуся файлу, второй файл, если он не менялся, остаётся как был показан (несохранённая
 * правка по нему не пропадает).
 */
/**
 * Перезагрузка, начатая до записи, которая случилась позже неё, читает уже устаревшее состояние
 * диска — требование 24. Её правда не в счёт: не откатывает показанное и не считается внешней
 * правкой, опрос сам найдёт свежую запись и перезагрузит ещё раз. `writeCountAtStart` — сколько раз
 * редактор записал файл до того, как эта перезагрузка начала читать (снимается в `useProjectEngine`
 * перед первым чтением); `currentWriteCount` — то же самое сейчас, когда её чтение дошло до применения.
 */
export function isReloadStale(writeCountAtStart: number, currentWriteCount: number): boolean {
  return currentWriteCount > writeCountAtStart;
}

export function applyExternalRead(state: EditSessionState, disk: EditSnapshot): EditSessionState {
  const sceneChanged = disk.sceneText !== state.diskTruth.sceneText;
  const propertiesChanged = disk.propertiesText !== state.diskTruth.propertiesText;
  const terrainChanged = disk.terrainText !== state.diskTruth.terrainText;
  const particlesChanged = disk.particlesText !== state.diskTruth.particlesText;
  const masksChanged = !areMaskSetsEqual(disk.masks, state.diskTruth.masks);
  if (!sceneChanged && !propertiesChanged && !terrainChanged && !particlesChanged && !masksChanged) return state;
  return {
    ...state,
    history: [...state.history, state.displayed],
    displayed: {
      sceneText: sceneChanged ? disk.sceneText : state.displayed.sceneText,
      propertiesText: propertiesChanged ? disk.propertiesText : state.displayed.propertiesText,
      terrainText: terrainChanged ? disk.terrainText : state.displayed.terrainText,
      particlesText: particlesChanged ? disk.particlesText : state.displayed.particlesText,
      masks: masksChanged ? disk.masks : state.displayed.masks,
    },
    diskTruth: disk,
  };
}
