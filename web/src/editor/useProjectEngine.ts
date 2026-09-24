import init, { Engine } from "engine";
import { useEffect, useRef, useState, type RefObject } from "react";
import { loadProject, type ProjectLoadResult } from "../projectLoader";
import { createCachingProjectFileReader, createOverridingReader } from "./cachingProjectFileReader";
import { watchFolderProject } from "./folderProjectWatcher";
import { listedProjectBaseUrl, createReaderForSource, type ProjectSource } from "./projectSource";
import { pollListedProject, type FileFingerprint } from "./listedProjectPoller";
import { parseProjectFilePaths } from "./projectFiles";
import { createRecordingProjectFileReader } from "./recordingProjectFileReader";
import { createReloadDebouncer } from "./reloadDebouncer";
import { createSerialQueue } from "./serialQueue";

/** Текст `scene.json`/`properties.json`, который правка держит в памяти вместо прочитанного с диска. */
export type EditedTexts = { sceneText?: string; propertiesText?: string };

export type ProjectEngineState = {
  engine: Engine | null;
  result: ProjectLoadResult | null;
  /** Когда пришёл `result` — последняя загрузка или перезагрузка после правки файлов. */
  loadedAt: Date | null;
  headerNotice: string | null;
  /** Отказ `init()`/`Engine.create` — тот же текст, что в этом случае показывает страница игры. */
  engineError: string | null;
  /**
   * Загрузка по правке — «Редактор», требование 1: те же три захода в тот же движок, но
   * `scene.json`/`properties.json` берутся из `edited`, а остальное — из кэша последней настоящей
   * загрузки, без сети и диска. `null` — движок ещё не завёлся или `game.json` ни разу не прочитан.
   */
  runEditedLoad: ((edited: EditedTexts) => Promise<ProjectLoadResult>) | null;
  /** Текст пути, который в последний раз прочитала настоящая (не по правке) загрузка. */
  getCachedText: (relativePath: string) => string | null | undefined;
  /** Отмечает путь как только что записанный — своя запись становится новой «правдой диска». */
  setCachedText: (relativePath: string, text: string) => void;
  /** Сколько раз редактор записал файл — требование 24, сравнивается с меткой из `onFullReload`. */
  getWriteCount: () => number;
};

function formatEngineBootError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Заводит движок один раз на открытый проект («Редактор», требование 21 — без клавиатуры, мыши и
 * `tick`), грузит его тремя заходами (требование 11) и переслушивает изменения файлов проекта
 * (требования 32–35): повторный `load()` идёт в тот же движок, а не создаёт новый — движок сам
 * заменяет прежнюю игру целиком (требование 20). `onFullReload` зовётся после каждой настоящей
 * (не по правке) загрузки — правка сама решает по её результату, внешняя ли это правка файлов
 * (требования 22–24). `writeCountAtStart` — сколько раз редактор успел записать файл до того, как
 * эта перезагрузка начала читать: перезагрузка, начатая до записи, которая случилась позже неё,
 * читает уже устаревшее состояние диска (требование 24).
 */
export function useProjectEngine(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  source: ProjectSource,
  onFullReload?: (
    result: ProjectLoadResult,
    getCachedText: (relativePath: string) => string | null | undefined,
    writeCountAtStart: number,
  ) => void,
): ProjectEngineState {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [result, setResult] = useState<ProjectLoadResult | null>(null);
  const [loadedAt, setLoadedAt] = useState<Date | null>(null);
  const [headerNotice, setHeaderNotice] = useState<string | null>(null);
  const [engineError, setEngineError] = useState<string | null>(null);
  const [editingApi, setEditingApi] = useState<Pick<ProjectEngineState, "runEditedLoad" | "getCachedText" | "setCachedText" | "getWriteCount">>({
    runEditedLoad: null,
    getCachedText: () => undefined,
    setCachedText: () => {},
    getWriteCount: () => 0,
  });

  // Обёртка над последним `onFullReload`, чтобы её не пришлось класть в зависимости эффекта —
  // иначе новая инлайн-функция от вызывающей стороны на каждом рендере перезаводила бы движок.
  const onFullReloadRef = useRef(onFullReload);
  useEffect(() => {
    onFullReloadRef.current = onFullReload;
  });

  useEffect(() => {
    let cancelled = false;
    let disposeWatch: (() => void) | null = null;
    let disposePoll: (() => void) | null = null;
    let disposeDebouncer: (() => void) | null = null;
    let createdEngine: Engine | null = null;
    let createdAudioContext: AudioContext | null = null;
    // Одна таблица путь → отпечаток на весь открытый проект («Редактор», требование 32): опрос
    // пересоздаётся на каждой загрузке, а эта таблица — нет, иначе отпечаток нового опроса не с
    // чем сравнить и правка между чтением файла загрузкой и первым тиком опроса теряется.
    const listedProjectFingerprints = new Map<string, FileFingerprint>();
    // Общая очередь на загрузки в этот движок — обычная перезагрузка и загрузка по правке
    // («Редактор», требование 6) никогда не идут наперегонки; каждая полностью заканчивается,
    // прежде чем начнётся следующая.
    const queue = createSerialQueue();
    // Движок и AudioContext закрываются только после того, как очередь загрузок сама опустеет —
    // закрыть их раньше значило бы позвать `read_entry`/`read_texts`/`load` уже на освобождённом
    // движке (необработанное исключение wasm-bindgen).
    let activeQueueTail: Promise<unknown> = Promise.resolve();

    async function boot(): Promise<void> {
      const canvas = canvasRef.current;
      if (!canvas) return;

      try {
        await init();
      } catch (error) {
        if (!cancelled) setEngineError(formatEngineBootError(error));
        return;
      }

      let engineInstance: Engine;
      try {
        engineInstance = await Engine.create(canvas);
      } catch (error) {
        if (!cancelled) setEngineError(formatEngineBootError(error));
        return;
      }
      if (cancelled) {
        engineInstance.free();
        return;
      }
      createdEngine = engineInstance;
      setEngine(engineInstance);

      const baseReader = createReaderForSource(source);
      const cache = createCachingProjectFileReader(baseReader);
      // Только у проекта из списка: опрос ходит ровно по путям, которые прочитала последняя
      // загрузка («Редактор», требование 32) — у папки с диска слежение даёт `FileSystemObserver`.
      const recordingReader = source.kind === "listed" ? createRecordingProjectFileReader(cache.reader) : null;
      const reader = recordingReader?.reader ?? cache.reader;
      const audioContext = new AudioContext();
      createdAudioContext = audioContext;
      // `game.json` последней настоящей загрузки — загрузка по правке берёт его отсюда, требование 1.
      let lastGameJsonText: string | null = null;

      async function reload(): Promise<void> {
        // Требование 24: метка снимается до первого чтения, а не после — иначе она уже включала бы
        // запись, которую действие делает, пока эта перезагрузка ещё читает с диска.
        const writeCountAtStart = cache.getWriteCount();
        recordingReader?.reset();
        const gameJsonText = await reader.readText("game.json");
        if (cancelled) return;
        lastGameJsonText = gameJsonText;
        let loadResult: ProjectLoadResult;
        if (gameJsonText === null) {
          loadResult = { status: "entry-missing" };
        } else {
          loadResult = await loadProject(engineInstance, reader, gameJsonText, () => audioContext);
          if (cancelled) return;
          // «Редактор», требование 16: собирает мир из сцены заново только на успешной загрузке —
          // без неё `show_scene` было бы нечего собирать, движок сам ничего не делает.
          if (loadResult.status === "ok") engineInstance.show_scene();
        }
        if (cancelled) return;
        setResult(loadResult);
        setLoadedAt(new Date());
        onFullReloadRef.current?.(loadResult, cache.getCachedText, writeCountAtStart);

        // Новая загрузка обновляет список опрашиваемых путей («Редактор», требование 32): опрос
        // перезапускается с новым списком, а таблица отпечатков — общая на весь проект — остаётся.
        if (recordingReader !== null && source.kind === "listed" && !cancelled) {
          disposePoll?.();
          disposePoll = pollListedProject(
            listedProjectBaseUrl(source),
            recordingReader.getReadPaths(),
            listedProjectFingerprints,
            debouncer.notify,
          );
        }
      }

      function runQueued<T>(task: () => Promise<T>): Promise<T> {
        const promise = queue.run(task);
        activeQueueTail = promise.catch(() => {});
        return promise;
      }

      function runReload(): Promise<void> {
        return runQueued(reload);
      }

      /**
       * Загрузка по правке — «Редактор», требование 1: пути `scene.json`/`properties.json` берутся
       * из последнего прочитанного `game.json` (как их назвал автор игры в `files`), сами тексты —
       * из `edited`, остальное — из кэша настоящей загрузки, без сети и диска.
       */
      async function runEditedLoad(edited: EditedTexts): Promise<ProjectLoadResult> {
        return runQueued(async () => {
          if (cancelled || lastGameJsonText === null) return { status: "entry-missing" } as ProjectLoadResult;
          const paths = parseProjectFilePaths(lastGameJsonText);
          if (paths === null) return { status: "entry-missing" } as ProjectLoadResult;
          const overrides: Record<string, string> = {};
          if (edited.sceneText !== undefined) overrides[paths.scene] = edited.sceneText;
          if (edited.propertiesText !== undefined) overrides[paths.properties] = edited.propertiesText;
          const overrideReader = createOverridingReader(cache.cachedReader, overrides);
          const loadResult = await loadProject(engineInstance, overrideReader, lastGameJsonText, () => audioContext);
          if (cancelled) return loadResult;
          if (loadResult.status === "ok") engineInstance.show_scene();
          setResult(loadResult);
          setLoadedAt(new Date());
          return loadResult;
        });
      }

      const debouncer = createReloadDebouncer(runReload);
      disposeDebouncer = debouncer.dispose;
      // Слежение за папкой с диска подключается сразу, ещё до конца первой загрузки («Редактор»,
      // требование 33) — изменение, сохранённое во время неё, не теряется: дребезг сам запускает
      // ещё одну перезагрузку следом за идущей.
      disposeWatch = source.kind === "folder" ? watchFolderProject(source.handle, debouncer.notify, setHeaderNotice) : () => {};

      setEditingApi({ runEditedLoad, getCachedText: cache.getCachedText, setCachedText: cache.setCachedText, getWriteCount: cache.getWriteCount });

      await debouncer.runNow();
    }

    void boot();

    return () => {
      cancelled = true;
      disposeWatch?.();
      disposePoll?.();
      disposeDebouncer?.();
      setEditingApi({ runEditedLoad: null, getCachedText: () => undefined, setCachedText: () => {}, getWriteCount: () => 0 });
      void activeQueueTail.finally(() => {
        createdEngine?.free();
        createdAudioContext?.close().catch(() => {});
      });
    };
  }, [canvasRef, source]);

  return { engine, result, loadedAt, headerNotice, engineError, ...editingApi };
}
