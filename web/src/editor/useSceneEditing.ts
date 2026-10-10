import type { Engine } from "engine";
import { useEffect, useMemo, useReducer, useRef, useState, type RefObject } from "react";
import type { ProjectLoadResult } from "../projectLoader";
import {
  applyExternalRead,
  beginAction,
  beginUndo,
  createEditSessionState,
  dirtyFiles,
  dirtyMaskPaths,
  isReloadStale,
  markUnsaved,
  markWritten,
  type EditSessionState,
  type EditSnapshot,
  type SaveState,
} from "./editSession";
import type { EditorCameraStore, FlatCameraStore } from "./editorCamera";
import { NO_MASKS, type MaskBytes, type MaskSet } from "./maskBytes";
import type { PlacementChange } from "./objectPlacement";
import type { PaintResult } from "./paintStroke";
import { encodeMaskPng } from "./pngCodec";
import type { PropertyKind } from "./propertiesDeclarations";
import { parseProjectFilePaths } from "./projectFiles";
import { writeProjectFile } from "./projectFileWriter";
import {
  addObjectProperty,
  appendSceneObject,
  declarePropertyKind,
  removeObjectProperty,
  removeSceneObject,
  setObjectPropertyValue,
  setObjectPropertyValues,
  sceneTextWithWind,
} from "./sceneTextEditing";
import { applyWind, type SceneWind } from "./sceneWind";
import {
  imprintsWithCopy,
  imprintsWithout,
  imprintsWithPlaced,
  imprintsWithReplaced,
  imprintsWithValue,
  type ImprintChange,
} from "./imprintEditing";
import { parseSceneObjects, resolveSelectionAfterReload, type SceneSize } from "./sceneObjects";
import { createSerialQueue } from "./serialQueue";
import type { ProjectSource } from "./projectSource";
import { planTerrainEdit, terrainTextAfterPaint } from "./terrainEditing";
import {
  readTerrainImprints,
  terrainTextWithHeights,
  terrainTextWithImprints,
  terrainTextWithWater,
  type ImprintEntry,
  type TerrainGrid,
  type TerrainWater,
} from "./terrainFile";
import { useProjectEngine } from "./useProjectEngine";

export type SceneEditingState = {
  engine: Engine | null;
  memory: WebAssembly.Memory | null;
  editorCameraStore: EditorCameraStore;
  flatCameraStore: FlatCameraStore;
  result: ProjectLoadResult | null;
  loadedAt: Date | null;
  headerNotice: string | null;
  engineError: string | null;
  hasQueuedReload: boolean;
  setReloadGateOpen: (isOpen: boolean) => void;
  /** Текст `scene.json`/`properties.json`, показанный сейчас — может быть несохранённым (требование 2). `null` — правка недоступна. */
  sceneText: string | null;
  propertiesText: string | null;
  /** Текст файла рельефа, показанный сейчас; `null` — у проекта нет файла рельефа. */
  terrainText: string | null;
  /** Маски слоёв покрытий, показанные сейчас, по путям файлов («Покраска»). */
  masks: MaskSet;
  saveState: SaveState;
  canUndo: boolean;
  /** Выбран либо один объект, либо один отпечаток: выбор одного снимает выбор другого («Редактор», «Сцена»). */
  selectedIndex: number | null;
  setSelectedIndex: (index: number | null) => void;
  /** Номер выбранного отпечатка в `stamps` файла рельефа; `null` — отпечаток не выбран. */
  selectedImprintIndex: number | null;
  setSelectedImprintIndex: (index: number | null) => void;
  undo: () => void;
  /** Ручки и перенос: все изменившиеся свойства объекта — одна правка текста и одна отмена. */
  transformObject: (objectIndex: number, changes: PlacementChange[]) => void;
  /** Кисти рельефа: мазок — высоты всей сетки, одно действие; первый мазок в проекте без рельефа заводит файл («Кисти рельефа», требования 18–19). */
  paintTerrain: (grid: TerrainGrid) => void;
  /** Покраска: слои покрытий и изменившиеся маски — одно действие; новый слой и первый мазок в проекте без рельефа заводят файлы («Покраска», требования 12, 15). */
  paintCovers: (result: PaintResult) => void;
  /** Мазок покраски брошен, а движок нечем вернуть вызовом: мир собирается заново из показанных файлов. */
  reloadDisplayed: () => void;
  /** Вода рельефа: `null` — воды нет; каждое принятое значение — одно действие (требование 23). */
  setTerrainWater: (water: TerrainWater | null) => void;
  /** Ветер сцены: движок проверяет его сразу, и без ошибки `wind` пишется в `scene.json` одним действием; с ошибкой файл не пишется, возвращается её текст («Правка сцены», требование 31). */
  setSceneWind: (wind: SceneWind) => string | undefined;
  /** Отпечатки — «Лепка рельефа»: каждое действие — одна правка файла рельефа, первый отпечаток в проекте без файла заводит его. */
  placeImprint: (entry: ImprintEntry) => void;
  replaceImprint: (index: number, entry: ImprintEntry) => void;
  /** Свойство отпечатка: `undefined` убирает ключ (пустой `rotation`). */
  setImprintValue: (index: number, key: string, value: unknown) => void;
  copyImprint: (index: number) => void;
  deleteImprint: (index: number) => void;
  setPropertyValue: (objectIndex: number, key: string, value: unknown) => void;
  removeProperty: (objectIndex: number, key: string) => void;
  /** Несколько свойств объекта одной правкой текста и одним шагом отмены — «Убрать» во вкладке «Эффекты» («Редактор», требование 31). */
  removeProperties: (objectIndex: number, keys: readonly string[]) => void;
  addProperty: (objectIndex: number, key: string, value: unknown) => void;
  declareProperty: (objectIndex: number, key: string, kind: PropertyKind, value: unknown) => void;
  copyObject: (objectIndex: number) => void;
  /** Новый объект в конец `objects` — одно действие; становится выбранным («Редактор», «Правка сцены», требование 25). */
  addObject: (object: Record<string, unknown>) => void;
  deleteObject: (objectIndex: number) => void;
  /** Исполняется, когда закончились все действия правки, поставленные в очередь до вызова, вместе с их загрузкой в движок. */
  whenIdle: () => Promise<void>;
};

/** «Редактор», требование 2: строка «Не сохранено — в проекте ошибки» — сами ошибки уже видны в панели ниже. */
const REJECTED_REASON = "в проекте ошибки";

const GAME_JSON_PATH = "game.json";

/** Маски, как их прочитала загрузка: путь → точки. */
function masksOfLoad(loaded: readonly { path: string; width: number; height: number; pixels: Uint8Array }[]): MaskSet {
  return Object.fromEntries(loaded.map(({ path, width, height, pixels }) => [path, { width, height, pixels }]));
}

function shiftCopiedPosition(object: Record<string, unknown>): Record<string, unknown> {
  const position = object.position;
  if (!Array.isArray(position) || typeof position[0] !== "number" || typeof position[1] !== "number") return object;
  return { ...object, position: [position[0] + 1, position[1], ...position.slice(2)] };
}

/**
 * Правка сцены — «Редактор», требования 1–26: заводит движок (`useProjectEngine`), держит показанный
 * текст `scene.json`/`properties.json`, историю и состояние записи, и даёт действия правки. Каждое
 * действие проверяется загрузкой по правке и, без ошибок, сразу пишет изменившиеся файлы.
 */
export function useSceneEditing(canvasRef: RefObject<HTMLCanvasElement | null>, source: ProjectSource): SceneEditingState {
  const [, forceRender] = useReducer((tick: number) => tick + 1, 0);
  const sessionRef = useRef<EditSessionState | null>(null);
  // `game.json` показанный и лежащий на диске: первый мазок дописывает в него `files.terrain` раньше записи.
  const gameJsonTextRef = useRef<string | null>(null);
  const diskGameJsonTextRef = useRef<string | null>(null);
  const [selectedIndex, setSelectedIndexState] = useState<number | null>(null);
  const [selectedImprintIndex, setSelectedImprintIndexState] = useState<number | null>(null);

  function updateSession(next: EditSessionState | null): void {
    sessionRef.current = next;
    forceRender();
  }

  // Своя очередь на действия правки и на переоценку после перезагрузки — требование 6: следующее
  // начинается после проверки и записи предыдущего, отмена и внешняя правка встают в ту же очередь.
  const actionQueue = useMemo(() => createSerialQueue(), [source]);

  const engineApiRef = useRef<Pick<ReturnType<typeof useProjectEngine>, "runEditedLoad" | "getCachedText" | "setCachedText" | "setCachedBytes" | "getWriteCount" | "isFilePresent">>({
    runEditedLoad: null,
    getCachedText: () => undefined,
    setCachedText: () => {},
    setCachedBytes: () => {},
    getWriteCount: () => 0,
    isFilePresent: () => Promise.resolve(false),
  });

  /**
   * Проверяет и, без ошибок, пишет изменившиеся файлы для текущего показанного текста — общий
   * хвост действия, отмены и переоценки после перезагрузки (требования 1–2, 23).
   */
  async function syncCurrent(): Promise<void> {
    const current = sessionRef.current;
    if (current === null) return;
    const runEditedLoad = engineApiRef.current.runEditedLoad;
    if (runEditedLoad === null) return;

    // Маски, что ещё не на диске, идут PNG и в проверку загрузкой, и в запись («Покраска», требование 12).
    const dirtyMasks = dirtyMaskPaths(current);
    const maskFiles: Record<string, Uint8Array> = {};
    for (const path of dirtyMasks) maskFiles[path] = await encodeMaskPng(current.displayed.masks[path] as MaskBytes);

    const result = await runEditedLoad({
      gameJsonText: gameJsonTextRef.current ?? undefined,
      sceneText: current.displayed.sceneText,
      propertiesText: current.displayed.propertiesText,
      terrainText: current.displayed.terrainText,
      maskFiles,
    });
    const afterLoad = sessionRef.current;
    if (afterLoad === null) return;

    if (result.status !== "ok") {
      updateSession(markUnsaved(afterLoad, REJECTED_REASON));
      return;
    }

    const paths = parseProjectFilePaths(gameJsonTextRef.current);
    const dirty = paths === null ? { scene: false, properties: false, terrain: false } : dirtyFiles(afterLoad);
    let failureReason: string | null = paths === null ? "не удалось определить пути файлов проекта" : null;

    // Маски раньше рельефа, а рельеф раньше `game.json`: слой не должен указывать на маску, а ключ `files.terrain` — на файл, которого ещё нет.
    const writes: { path: string | null | undefined; text: string | Uint8Array | null; isDirty: boolean }[] = [
      ...dirtyMasks.map((path) => ({ path, text: maskFiles[path] as Uint8Array, isDirty: true })),
      { path: paths?.scene, text: afterLoad.displayed.sceneText, isDirty: dirty.scene },
      { path: paths?.properties, text: afterLoad.displayed.propertiesText, isDirty: dirty.properties },
      { path: paths?.terrain, text: afterLoad.displayed.terrainText, isDirty: dirty.terrain },
      { path: GAME_JSON_PATH, text: gameJsonTextRef.current, isDirty: gameJsonTextRef.current !== diskGameJsonTextRef.current },
    ];
    for (const write of writes) {
      if (!write.isDirty || failureReason !== null || write.path == null || write.text === null) continue;
      const writeResult = await writeProjectFile(source, write.path, write.text);
      if (!writeResult.ok) {
        failureReason = writeResult.reason;
        continue;
      }
      if (typeof write.text !== "string") {
        engineApiRef.current.setCachedBytes(write.path, write.text);
        continue;
      }
      engineApiRef.current.setCachedText(write.path, write.text);
      if (write.path === GAME_JSON_PATH) diskGameJsonTextRef.current = write.text;
    }

    const afterWrite = sessionRef.current;
    if (afterWrite === null) return;
    updateSession(failureReason === null ? markWritten(afterWrite) : markUnsaved(afterWrite, failureReason));
  }

  /** Одно действие правки — требование 6: вычисляет новый текст, показывает его сразу, потом проверяет и пишет. */
  function dispatchEdit(build: (displayed: EditSnapshot) => EditSnapshot | null): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      if (current === null) return;
      const candidate = build(current.displayed);
      if (candidate === null) return;
      const { sceneText, propertiesText, terrainText } = current.displayed;
      if (candidate.sceneText === sceneText && candidate.propertiesText === propertiesText && candidate.terrainText === terrainText) return;
      updateSession(beginAction(current, candidate));
      await syncCurrent();
    });
  }

  function handleFullReload(
    result: ProjectLoadResult,
    getCachedText: (relativePath: string) => string | null | undefined,
    writeCountAtStart: number,
  ): void {
    void actionQueue.run(async () => {
      // Устаревшая перезагрузка уже отдала движку и панели ошибок старый текст с диска — возвращаем
      // им показанный: опрос может и не заметить свою запись той же длины в ту же секунду.
      if (isReloadStale(writeCountAtStart, engineApiRef.current.getWriteCount())) {
        await syncCurrent();
        return;
      }
      if (result.status === "entry-missing") {
        gameJsonTextRef.current = null;
        diskGameJsonTextRef.current = null;
        updateSession(null);
        return;
      }
      // Показанный `game.json` может опережать диск (ключ рельефа, что ещё не записан): чужая правка `game.json` его заменяет, своя нет.
      if (result.gameJsonText !== diskGameJsonTextRef.current) gameJsonTextRef.current = result.gameJsonText;
      diskGameJsonTextRef.current = result.gameJsonText;
      const paths = parseProjectFilePaths(result.gameJsonText);
      const sceneText = result.sceneText;
      const propertiesText = paths === null ? null : (getCachedText(paths.properties) ?? null);
      const terrainText = paths?.terrain == null ? null : (getCachedText(paths.terrain) ?? null);
      if (paths === null || sceneText === null || propertiesText === null || (paths.terrain !== null && terrainText === null)) {
        updateSession(null);
        return;
      }
      // Маски читает сама загрузка; отказавшая загрузка их не отдаёт — тогда по ним прежняя «правда диска».
      const masks = result.status === "ok" ? masksOfLoad(result.coverMasks) : (sessionRef.current?.diskTruth.masks ?? NO_MASKS);
      const disk: EditSnapshot = { sceneText, propertiesText, terrainText, masks };
      const current = sessionRef.current;
      if (current === null) {
        updateSession(createEditSessionState(disk));
        return;
      }
      updateSession(applyExternalRead(current, disk));
      // Требование 23: несохранённая правка (или только что показанная внешняя правка) проверяется
      // заново вместе со свежими файлами и пишется, если ошибок больше нет.
      await syncCurrent();
    });
  }

  const engineState = useProjectEngine(canvasRef, source, handleFullReload);
  useEffect(() => {
    engineApiRef.current = {
      runEditedLoad: engineState.runEditedLoad,
      getCachedText: engineState.getCachedText,
      setCachedText: engineState.setCachedText,
      setCachedBytes: engineState.setCachedBytes,
      getWriteCount: engineState.getWriteCount,
      isFilePresent: engineState.isFilePresent,
    };
  });

  // Проект сменился — своя история и показанный текст не переживают его («Редактор», требование 21).
  useEffect(() => {
    sessionRef.current = null;
    gameJsonTextRef.current = null;
    diskGameJsonTextRef.current = null;
    setSelectedIndexState(null);
    setSelectedImprintIndexState(null);
    forceRender();
  }, [source]);

  const session = sessionRef.current;
  const sceneText = session?.displayed.sceneText ?? null;
  const terrainText = session?.displayed.terrainText ?? null;
  const imprintCount = useMemo(() => readTerrainImprints(terrainText).length, [terrainText]);

  function selectObject(index: number | null): void {
    setSelectedIndexState(index);
    setSelectedImprintIndexState(null);
  }

  function selectImprint(index: number | null): void {
    setSelectedImprintIndexState(index);
    if (index !== null) setSelectedIndexState(null);
  }

  // После отмены и внешней правки — выбор на том же номере, если он есть, иначе снят («Редактор»,
  // требование 25). Копия и удаление уже поставили свой номер сами — здесь их только не трогает:
  // после них номер либо в границах (копия), либо не выбран (удаление), clamp тогда не меняет ничего.
  useEffect(() => {
    setSelectedIndexState((current) => resolveSelectionAfterReload(current, parseSceneObjects(sceneText).length));
  }, [sceneText]);

  // Отпечаток после отмены и внешней правки — тот же номер, если такой отпечаток ещё есть («Лепка рельефа», «Редактор», требование 32).
  useEffect(() => {
    setSelectedImprintIndexState((current) => resolveSelectionAfterReload(current, imprintCount));
  }, [imprintCount]);

  function undo(): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      if (current === null) return;
      const popped = beginUndo(current);
      if (popped === null) return;
      updateSession(popped.state);
      await syncCurrent();
    });
  }

  function transformObject(objectIndex: number, changes: PlacementChange[]): void {
    dispatchEdit((displayed) => {
      const object = parseSceneObjects(displayed.sceneText)[objectIndex];
      if (object === undefined || object === null || typeof object !== "object" || Array.isArray(object)) return null;
      const values = Object.fromEntries(changes.map((change) => [change.key, change.value]));
      return { ...displayed, sceneText: setObjectPropertyValues(displayed.sceneText, objectIndex, values) };
    });
  }

  /** Действие с рельефом: проект без файла рельефа получает его первым действием («Кисти рельефа», требование 19). */
  function dispatchTerrainEdit(build: (displayed: EditSnapshot, sceneSize: SceneSize) => string | null, maskChanges?: MaskSet): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      const gameJsonText = gameJsonTextRef.current;
      if (current === null || gameJsonText === null) return;
      const plan = await planTerrainEdit(current, gameJsonText, engineApiRef.current.isFilePresent, build, maskChanges);
      if (plan === null) return;
      gameJsonTextRef.current = plan.gameJsonText;
      updateSession(plan.state);
      await syncCurrent();
    });
  }

  function paintTerrain(grid: TerrainGrid): void {
    dispatchTerrainEdit((displayed) => terrainTextWithHeights(displayed.terrainText, grid));
  }

  function paintCovers(result: PaintResult): void {
    dispatchTerrainEdit((displayed, sceneSize) => terrainTextAfterPaint(displayed.terrainText, sceneSize, result.covers), result.masks);
  }

  function reloadDisplayed(): void {
    void actionQueue.run(syncCurrent);
  }

  function setTerrainWater(water: TerrainWater | null): void {
    dispatchTerrainEdit((displayed, sceneSize) => terrainTextWithWater(displayed.terrainText, sceneSize, water));
  }

  function setSceneWind(wind: SceneWind): string | undefined {
    const engine = engineState.engine;
    if (engine === null) return undefined;
    // Проверка идёт сразу, до очереди: ветер пары чисел от мира не зависит, а ошибка нужна окошку сейчас.
    const error = applyWind(engine, wind);
    if (error !== undefined) return error;
    dispatchEdit((displayed) => ({ ...displayed, sceneText: sceneTextWithWind(displayed.sceneText, wind) }));
    return undefined;
  }

  /**
   * Действие с отпечатками: `change` получает отпечатки файла и отдаёт новый список, `null` — действия нет. Выбор
   * сдвигается вместе с действием, когда оно его просит (новый отпечаток, копия, удаление).
   */
  function dispatchImprintEdit(change: (imprints: ImprintEntry[]) => ImprintChange | null): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      const gameJsonText = gameJsonTextRef.current;
      if (current === null || gameJsonText === null) return;
      const outcome: { selected?: number | null } = {};
      const plan = await planTerrainEdit(current, gameJsonText, engineApiRef.current.isFilePresent, (displayed, sceneSize) => {
        const changed = change(readTerrainImprints(displayed.terrainText));
        if (changed === null) return null;
        outcome.selected = changed.selected;
        return terrainTextWithImprints(displayed.terrainText, sceneSize, changed.imprints);
      });
      if (plan === null) return;
      gameJsonTextRef.current = plan.gameJsonText;
      updateSession(plan.state);
      if (outcome.selected !== undefined) selectImprint(outcome.selected);
      await syncCurrent();
    });
  }

  function placeImprint(entry: ImprintEntry): void {
    dispatchImprintEdit((imprints) => imprintsWithPlaced(imprints, entry));
  }

  function replaceImprint(index: number, entry: ImprintEntry): void {
    dispatchImprintEdit((imprints) => imprintsWithReplaced(imprints, index, entry));
  }

  function setImprintValue(index: number, key: string, value: unknown): void {
    dispatchImprintEdit((imprints) => imprintsWithValue(imprints, index, key, value));
  }

  function copyImprint(index: number): void {
    dispatchImprintEdit((imprints) => imprintsWithCopy(imprints, index));
  }

  function deleteImprint(index: number): void {
    dispatchImprintEdit((imprints) => imprintsWithout(imprints, index));
  }

  function setPropertyValue(objectIndex: number, key: string, value: unknown): void {
    dispatchEdit((displayed) => {
      const objects = parseSceneObjects(displayed.sceneText);
      const object = objects[objectIndex] as Record<string, unknown> | undefined;
      if (object === undefined || JSON.stringify(object[key]) === JSON.stringify(value)) return null;
      return { ...displayed, sceneText: setObjectPropertyValue(displayed.sceneText, objectIndex, key, value) };
    });
  }

  function removeProperty(objectIndex: number, key: string): void {
    dispatchEdit((displayed) => ({ ...displayed, sceneText: removeObjectProperty(displayed.sceneText, objectIndex, key) }));
  }

  function removeProperties(objectIndex: number, keys: readonly string[]): void {
    dispatchEdit((displayed) => ({ ...displayed, sceneText: keys.reduce((text, key) => removeObjectProperty(text, objectIndex, key), displayed.sceneText) }));
  }

  function addProperty(objectIndex: number, key: string, value: unknown): void {
    dispatchEdit((displayed) => ({ ...displayed, sceneText: addObjectProperty(displayed.sceneText, objectIndex, key, value) }));
  }

  function declareProperty(objectIndex: number, key: string, kind: PropertyKind, value: unknown): void {
    dispatchEdit((displayed) => ({
      ...displayed,
      sceneText: addObjectProperty(displayed.sceneText, objectIndex, key, value),
      propertiesText: declarePropertyKind(displayed.propertiesText, key, kind),
    }));
  }

  function copyObject(objectIndex: number): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      if (current === null) return;
      const objects = parseSceneObjects(current.displayed.sceneText);
      const object = objects[objectIndex];
      if (object === undefined) return;
      const copy = object !== null && typeof object === "object" && !Array.isArray(object) ? shiftCopiedPosition(object as Record<string, unknown>) : object;
      const candidate: EditSnapshot = { ...current.displayed, sceneText: appendSceneObject(current.displayed.sceneText, objects.length, copy) };
      updateSession(beginAction(current, candidate));
      setSelectedIndexState(objects.length);
      await syncCurrent();
    });
  }

  function addObject(object: Record<string, unknown>): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      if (current === null) return;
      const count = parseSceneObjects(current.displayed.sceneText).length;
      const candidate: EditSnapshot = { ...current.displayed, sceneText: appendSceneObject(current.displayed.sceneText, count, object) };
      updateSession(beginAction(current, candidate));
      selectObject(count);
      await syncCurrent();
    });
  }

  function deleteObject(objectIndex: number): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      if (current === null) return;
      const objects = parseSceneObjects(current.displayed.sceneText);
      if (objects[objectIndex] === undefined) return;
      const candidate: EditSnapshot = { ...current.displayed, sceneText: removeSceneObject(current.displayed.sceneText, objectIndex) };
      updateSession(beginAction(current, candidate));
      setSelectedIndexState(null);
      await syncCurrent();
    });
  }

  function whenIdle(): Promise<void> {
    return actionQueue.run(() => Promise.resolve());
  }

  return {
    engine: engineState.engine,
    memory: engineState.memory,
    editorCameraStore: engineState.editorCameraStore,
    flatCameraStore: engineState.flatCameraStore,
    result: engineState.result,
    loadedAt: engineState.loadedAt,
    headerNotice: engineState.headerNotice,
    engineError: engineState.engineError,
    hasQueuedReload: engineState.hasQueuedReload,
    setReloadGateOpen: engineState.setReloadGateOpen,
    sceneText: session?.displayed.sceneText ?? null,
    propertiesText: session?.displayed.propertiesText ?? null,
    terrainText,
    masks: session?.displayed.masks ?? NO_MASKS,
    saveState: session?.saveState ?? { status: "saved" },
    canUndo: (session?.history.length ?? 0) > 0,
    selectedIndex,
    setSelectedIndex: selectObject,
    selectedImprintIndex,
    setSelectedImprintIndex: selectImprint,
    undo,
    transformObject,
    paintTerrain,
    paintCovers,
    reloadDisplayed,
    setTerrainWater,
    setSceneWind,
    placeImprint,
    replaceImprint,
    setImprintValue,
    copyImprint,
    deleteImprint,
    setPropertyValue,
    removeProperty,
    removeProperties,
    addProperty,
    declareProperty,
    copyObject,
    addObject,
    deleteObject,
    whenIdle,
  };
}
