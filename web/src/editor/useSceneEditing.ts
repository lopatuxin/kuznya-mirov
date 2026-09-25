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
import type { PropertyKind } from "./propertiesDeclarations";
import { parseProjectFilePaths } from "./projectFiles";
import { writeProjectFile } from "./projectFileWriter";
import { addObjectProperty, appendSceneObject, declarePropertyKind, removeObjectProperty, removeSceneObject, setObjectPropertyValue } from "./sceneTextEditing";
import { parseSceneObjects, resolveSelectionAfterReload } from "./sceneObjects";
import { createSerialQueue } from "./serialQueue";
import type { ProjectSource } from "./projectSource";
import { useProjectEngine } from "./useProjectEngine";

export type SceneEditingState = {
  engine: Engine | null;
  memory: WebAssembly.Memory | null;
  result: ProjectLoadResult | null;
  loadedAt: Date | null;
  headerNotice: string | null;
  engineError: string | null;
  hasQueuedReload: boolean;
  setReloadGateOpen: (isOpen: boolean) => void;
  /** Текст `scene.json`/`properties.json`, показанный сейчас — может быть несохранённым (требование 2). `null` — правка недоступна. */
  sceneText: string | null;
  propertiesText: string | null;
  saveState: SaveState;
  canUndo: boolean;
  selectedIndex: number | null;
  setSelectedIndex: (index: number | null) => void;
  undo: () => void;
  moveObject: (objectIndex: number, position: readonly [number, number]) => void;
  setPropertyValue: (objectIndex: number, key: string, value: unknown) => void;
  removeProperty: (objectIndex: number, key: string) => void;
  addProperty: (objectIndex: number, key: string, value: unknown) => void;
  declareProperty: (objectIndex: number, key: string, kind: PropertyKind, value: unknown) => void;
  copyObject: (objectIndex: number) => void;
  deleteObject: (objectIndex: number) => void;
};

/** «Редактор», требование 2: строка «Не сохранено — в проекте ошибки» — сами ошибки уже видны в панели ниже. */
const REJECTED_REASON = "в проекте ошибки";

function shiftCopiedPosition(object: Record<string, unknown>): Record<string, unknown> {
  const position = object.position;
  if (!Array.isArray(position) || typeof position[0] !== "number" || typeof position[1] !== "number") return object;
  return { ...object, position: [position[0] + 1, position[1]] };
}

/**
 * Правка сцены — «Редактор», требования 1–26: заводит движок (`useProjectEngine`), держит показанный
 * текст `scene.json`/`properties.json`, историю и состояние записи, и даёт действия правки. Каждое
 * действие проверяется загрузкой по правке и, без ошибок, сразу пишет изменившиеся файлы.
 */
export function useSceneEditing(canvasRef: RefObject<HTMLCanvasElement | null>, source: ProjectSource): SceneEditingState {
  const [, forceRender] = useReducer((tick: number) => tick + 1, 0);
  const sessionRef = useRef<EditSessionState | null>(null);
  const gameJsonTextRef = useRef<string | null>(null);
  const [selectedIndex, setSelectedIndexState] = useState<number | null>(null);

  function updateSession(next: EditSessionState | null): void {
    sessionRef.current = next;
    forceRender();
  }

  // Своя очередь на действия правки и на переоценку после перезагрузки — требование 6: следующее
  // начинается после проверки и записи предыдущего, отмена и внешняя правка встают в ту же очередь.
  const actionQueue = useMemo(() => createSerialQueue(), [source]);

  const engineApiRef = useRef<Pick<ReturnType<typeof useProjectEngine>, "runEditedLoad" | "getCachedText" | "setCachedText" | "getWriteCount">>({
    runEditedLoad: null,
    getCachedText: () => undefined,
    setCachedText: () => {},
    getWriteCount: () => 0,
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

    const result = await runEditedLoad({ sceneText: current.displayed.sceneText, propertiesText: current.displayed.propertiesText });
    const afterLoad = sessionRef.current;
    if (afterLoad === null) return;

    if (result.status !== "ok") {
      updateSession(markUnsaved(afterLoad, REJECTED_REASON));
      return;
    }

    const paths = parseProjectFilePaths(gameJsonTextRef.current);
    const dirty = paths === null ? { scene: false, properties: false } : dirtyFiles(afterLoad);
    let failureReason: string | null = paths === null ? "не удалось определить пути файлов проекта" : null;

    if (paths !== null && dirty.scene) {
      const writeResult = await writeProjectFile(source, paths.scene, afterLoad.displayed.sceneText);
      if (writeResult.ok) engineApiRef.current.setCachedText(paths.scene, afterLoad.displayed.sceneText);
      else failureReason = writeResult.reason;
    }
    if (paths !== null && dirty.properties && failureReason === null) {
      const writeResult = await writeProjectFile(source, paths.properties, afterLoad.displayed.propertiesText);
      if (writeResult.ok) engineApiRef.current.setCachedText(paths.properties, afterLoad.displayed.propertiesText);
      else failureReason = writeResult.reason;
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
      if (candidate.sceneText === current.displayed.sceneText && candidate.propertiesText === current.displayed.propertiesText) return;
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
        updateSession(null);
        return;
      }
      gameJsonTextRef.current = result.gameJsonText;
      const paths = parseProjectFilePaths(result.gameJsonText);
      const sceneText = result.sceneText;
      const propertiesText = paths === null ? null : (getCachedText(paths.properties) ?? null);
      if (paths === null || sceneText === null || propertiesText === null) {
        updateSession(null);
        return;
      }
      const disk: EditSnapshot = { sceneText, propertiesText };
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
    };
  });

  // Проект сменился — своя история и показанный текст не переживают его («Редактор», требование 21).
  useEffect(() => {
    sessionRef.current = null;
    gameJsonTextRef.current = null;
    setSelectedIndexState(null);
    forceRender();
  }, [source]);

  const session = sessionRef.current;
  const sceneText = session?.displayed.sceneText ?? null;

  // После отмены и внешней правки — выбор на том же номере, если он есть, иначе снят («Редактор»,
  // требование 25). Копия и удаление уже поставили свой номер сами — здесь их только не трогает:
  // после них номер либо в границах (копия), либо не выбран (удаление), clamp тогда не меняет ничего.
  useEffect(() => {
    setSelectedIndexState((current) => resolveSelectionAfterReload(current, parseSceneObjects(sceneText).length));
  }, [sceneText]);

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
      return { sceneText: setObjectPropertyValue(displayed.sceneText, objectIndex, "position", [position[0], position[1]]), propertiesText: displayed.propertiesText };
    });
  }

  function setPropertyValue(objectIndex: number, key: string, value: unknown): void {
    dispatchEdit((displayed) => {
      const objects = parseSceneObjects(displayed.sceneText);
      const object = objects[objectIndex] as Record<string, unknown> | undefined;
      if (object === undefined || JSON.stringify(object[key]) === JSON.stringify(value)) return null;
      return { sceneText: setObjectPropertyValue(displayed.sceneText, objectIndex, key, value), propertiesText: displayed.propertiesText };
    });
  }

  function removeProperty(objectIndex: number, key: string): void {
    dispatchEdit((displayed) => ({ sceneText: removeObjectProperty(displayed.sceneText, objectIndex, key), propertiesText: displayed.propertiesText }));
  }

  function addProperty(objectIndex: number, key: string, value: unknown): void {
    dispatchEdit((displayed) => ({ sceneText: addObjectProperty(displayed.sceneText, objectIndex, key, value), propertiesText: displayed.propertiesText }));
  }

  function declareProperty(objectIndex: number, key: string, kind: PropertyKind, value: unknown): void {
    dispatchEdit((displayed) => ({
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
      const candidate: EditSnapshot = { sceneText: appendSceneObject(current.displayed.sceneText, objects.length, copy), propertiesText: current.displayed.propertiesText };
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
      const candidate: EditSnapshot = { sceneText: removeSceneObject(current.displayed.sceneText, objectIndex), propertiesText: current.displayed.propertiesText };
      updateSession(beginAction(current, candidate));
      setSelectedIndexState(null);
      await syncCurrent();
    });
  }

  return {
    engine: engineState.engine,
    memory: engineState.memory,
    result: engineState.result,
    loadedAt: engineState.loadedAt,
    headerNotice: engineState.headerNotice,
    engineError: engineState.engineError,
    hasQueuedReload: engineState.hasQueuedReload,
    setReloadGateOpen: engineState.setReloadGateOpen,
    sceneText: session?.displayed.sceneText ?? null,
    propertiesText: session?.displayed.propertiesText ?? null,
    saveState: session?.saveState ?? { status: "saved" },
    canUndo: (session?.history.length ?? 0) > 0,
    selectedIndex,
    setSelectedIndex: setSelectedIndexState,
    undo,
    moveObject,
    setPropertyValue,
    removeProperty,
    addProperty,
    declareProperty,
    copyObject,
    deleteObject,
  };
}
