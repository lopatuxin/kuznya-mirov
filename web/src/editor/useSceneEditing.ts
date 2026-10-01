import type { Engine } from "engine";
import { useEffect, useMemo, useReducer, useRef, useState, type RefObject } from "react";
import type { ProjectLoadResult } from "../projectLoader";
import {
  applyExternalRead,
  beginAction,
  beginUndo,
  createEditSessionState,
  dirtyFiles,
  isReloadStale,
  markUnsaved,
  markWritten,
  type EditSessionState,
  type EditSnapshot,
  type SaveState,
} from "./editSession";
import type { EditorCameraStore } from "./editorCamera";
import type { PlacementChange } from "./objectPlacement";
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
} from "./sceneTextEditing";
import {
  mountainsWithCopy,
  mountainsWithout,
  mountainsWithPlaced,
  mountainsWithReplaced,
  mountainsWithValue,
  type MountainChange,
} from "./mountainEditing";
import { parseSceneObjects, resolveSelectionAfterReload, type SceneSize } from "./sceneObjects";
import { createSerialQueue } from "./serialQueue";
import type { ProjectSource } from "./projectSource";
import { planTerrainEdit } from "./terrainEditing";
import {
  readTerrainMountains,
  terrainTextWithHeights,
  terrainTextWithMountains,
  terrainTextWithWater,
  type MountainEntry,
  type TerrainGrid,
  type TerrainWater,
} from "./terrainFile";
import { useProjectEngine } from "./useProjectEngine";

export type SceneEditingState = {
  engine: Engine | null;
  memory: WebAssembly.Memory | null;
  editorCameraStore: EditorCameraStore;
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
  saveState: SaveState;
  canUndo: boolean;
  /** Выбран либо один объект, либо одна гора: выбор одного снимает выбор другого («Редактор», «Сцена»). */
  selectedIndex: number | null;
  setSelectedIndex: (index: number | null) => void;
  /** Номер выбранной горы в `stamps` файла рельефа; `null` — гора не выбрана. */
  selectedMountainIndex: number | null;
  setSelectedMountainIndex: (index: number | null) => void;
  undo: () => void;
  moveObject: (objectIndex: number, position: readonly [number, number]) => void;
  /** Ручки трёхмерной сцены: все изменившиеся свойства объекта — одна правка текста и одна отмена. */
  transformObject: (objectIndex: number, changes: PlacementChange[]) => void;
  /** Кисти рельефа: мазок — высоты всей сетки, одно действие; первый мазок в проекте без рельефа заводит файл («Кисти рельефа», требования 18–19). */
  paintTerrain: (grid: TerrainGrid) => void;
  /** Вода рельефа: `null` — воды нет; каждое принятое значение — одно действие (требование 23). */
  setTerrainWater: (water: TerrainWater | null) => void;
  /** Горы — «Лепка рельефа»: каждое действие — одна правка файла рельефа, первая гора в проекте без файла заводит его. */
  placeMountain: (entry: MountainEntry) => void;
  replaceMountain: (index: number, entry: MountainEntry) => void;
  /** Свойство горы: `undefined` убирает ключ (пустой `rotation`). */
  setMountainValue: (index: number, key: string, value: unknown) => void;
  copyMountain: (index: number) => void;
  deleteMountain: (index: number) => void;
  setPropertyValue: (objectIndex: number, key: string, value: unknown) => void;
  removeProperty: (objectIndex: number, key: string) => void;
  addProperty: (objectIndex: number, key: string, value: unknown) => void;
  declareProperty: (objectIndex: number, key: string, kind: PropertyKind, value: unknown) => void;
  copyObject: (objectIndex: number) => void;
  deleteObject: (objectIndex: number) => void;
};

/** «Редактор», требование 2: строка «Не сохранено — в проекте ошибки» — сами ошибки уже видны в панели ниже. */
const REJECTED_REASON = "в проекте ошибки";

const GAME_JSON_PATH = "game.json";

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
  const [selectedMountainIndex, setSelectedMountainIndexState] = useState<number | null>(null);

  function updateSession(next: EditSessionState | null): void {
    sessionRef.current = next;
    forceRender();
  }

  // Своя очередь на действия правки и на переоценку после перезагрузки — требование 6: следующее
  // начинается после проверки и записи предыдущего, отмена и внешняя правка встают в ту же очередь.
  const actionQueue = useMemo(() => createSerialQueue(), [source]);

  const engineApiRef = useRef<Pick<ReturnType<typeof useProjectEngine>, "runEditedLoad" | "getCachedText" | "setCachedText" | "getWriteCount" | "isFilePresent">>({
    runEditedLoad: null,
    getCachedText: () => undefined,
    setCachedText: () => {},
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

    const result = await runEditedLoad({
      gameJsonText: gameJsonTextRef.current ?? undefined,
      sceneText: current.displayed.sceneText,
      propertiesText: current.displayed.propertiesText,
      terrainText: current.displayed.terrainText,
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

    // Рельеф раньше `game.json`: ключ `files.terrain` не должен указывать на файл, которого ещё нет.
    const writes = [
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
      const disk: EditSnapshot = { sceneText, propertiesText, terrainText };
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
    setSelectedMountainIndexState(null);
    forceRender();
  }, [source]);

  const session = sessionRef.current;
  const sceneText = session?.displayed.sceneText ?? null;
  const terrainText = session?.displayed.terrainText ?? null;
  const mountainCount = useMemo(() => readTerrainMountains(terrainText).length, [terrainText]);

  function selectObject(index: number | null): void {
    setSelectedIndexState(index);
    setSelectedMountainIndexState(null);
  }

  function selectMountain(index: number | null): void {
    setSelectedMountainIndexState(index);
    if (index !== null) setSelectedIndexState(null);
  }

  // После отмены и внешней правки — выбор на том же номере, если он есть, иначе снят («Редактор»,
  // требование 25). Копия и удаление уже поставили свой номер сами — здесь их только не трогает:
  // после них номер либо в границах (копия), либо не выбран (удаление), clamp тогда не меняет ничего.
  useEffect(() => {
    setSelectedIndexState((current) => resolveSelectionAfterReload(current, parseSceneObjects(sceneText).length));
  }, [sceneText]);

  // Гора после отмены и внешней правки — тот же номер, если такая гора ещё есть («Лепка рельефа», «Редактор», требование 32).
  useEffect(() => {
    setSelectedMountainIndexState((current) => resolveSelectionAfterReload(current, mountainCount));
  }, [mountainCount]);

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

  function moveObject(objectIndex: number, position: readonly [number, number]): void {
    dispatchEdit((displayed) => {
      const objects = parseSceneObjects(displayed.sceneText);
      const object = objects[objectIndex];
      if (object === undefined || object === null || typeof object !== "object" || Array.isArray(object)) return null;
      const current = (object as Record<string, unknown>).position;
      if (Array.isArray(current) && current[0] === position[0] && current[1] === position[1]) return null;
      return { ...displayed, sceneText: setObjectPropertyValue(displayed.sceneText, objectIndex, "position", [position[0], position[1]]) };
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
  function dispatchTerrainEdit(build: (displayed: EditSnapshot, sceneSize: SceneSize) => string | null): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      const gameJsonText = gameJsonTextRef.current;
      if (current === null || gameJsonText === null) return;
      const plan = await planTerrainEdit(current, gameJsonText, engineApiRef.current.isFilePresent, build);
      if (plan === null) return;
      gameJsonTextRef.current = plan.gameJsonText;
      updateSession(plan.state);
      await syncCurrent();
    });
  }

  function paintTerrain(grid: TerrainGrid): void {
    dispatchTerrainEdit((displayed) => terrainTextWithHeights(displayed.terrainText, grid));
  }

  function setTerrainWater(water: TerrainWater | null): void {
    dispatchTerrainEdit((displayed, sceneSize) => terrainTextWithWater(displayed.terrainText, sceneSize, water));
  }

  /**
   * Действие с горами: `change` получает горы файла и отдаёт новый список, `null` — действия нет. Выбор
   * сдвигается вместе с действием, когда оно его просит (новая гора, копия, удаление).
   */
  function dispatchMountainEdit(change: (mountains: MountainEntry[]) => MountainChange | null): void {
    void actionQueue.run(async () => {
      const current = sessionRef.current;
      const gameJsonText = gameJsonTextRef.current;
      if (current === null || gameJsonText === null) return;
      const outcome: { selected?: number | null } = {};
      const plan = await planTerrainEdit(current, gameJsonText, engineApiRef.current.isFilePresent, (displayed, sceneSize) => {
        const changed = change(readTerrainMountains(displayed.terrainText));
        if (changed === null) return null;
        outcome.selected = changed.selected;
        return terrainTextWithMountains(displayed.terrainText, sceneSize, changed.mountains);
      });
      if (plan === null) return;
      gameJsonTextRef.current = plan.gameJsonText;
      updateSession(plan.state);
      if (outcome.selected !== undefined) selectMountain(outcome.selected);
      await syncCurrent();
    });
  }

  function placeMountain(entry: MountainEntry): void {
    dispatchMountainEdit((mountains) => mountainsWithPlaced(mountains, entry));
  }

  function replaceMountain(index: number, entry: MountainEntry): void {
    dispatchMountainEdit((mountains) => mountainsWithReplaced(mountains, index, entry));
  }

  function setMountainValue(index: number, key: string, value: unknown): void {
    dispatchMountainEdit((mountains) => mountainsWithValue(mountains, index, key, value));
  }

  function copyMountain(index: number): void {
    dispatchMountainEdit((mountains) => mountainsWithCopy(mountains, index));
  }

  function deleteMountain(index: number): void {
    dispatchMountainEdit((mountains) => mountainsWithout(mountains, index));
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

  return {
    engine: engineState.engine,
    memory: engineState.memory,
    editorCameraStore: engineState.editorCameraStore,
    result: engineState.result,
    loadedAt: engineState.loadedAt,
    headerNotice: engineState.headerNotice,
    engineError: engineState.engineError,
    hasQueuedReload: engineState.hasQueuedReload,
    setReloadGateOpen: engineState.setReloadGateOpen,
    sceneText: session?.displayed.sceneText ?? null,
    propertiesText: session?.displayed.propertiesText ?? null,
    terrainText,
    saveState: session?.saveState ?? { status: "saved" },
    canUndo: (session?.history.length ?? 0) > 0,
    selectedIndex,
    setSelectedIndex: selectObject,
    selectedMountainIndex,
    setSelectedMountainIndex: selectMountain,
    undo,
    moveObject,
    transformObject,
    paintTerrain,
    setTerrainWater,
    placeMountain,
    replaceMountain,
    setMountainValue,
    copyMountain,
    deleteMountain,
    setPropertyValue,
    removeProperty,
    addProperty,
    declareProperty,
    copyObject,
    deleteObject,
  };
}
