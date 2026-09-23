import init, { Engine } from "engine";
import { useEffect, useState, type RefObject } from "react";
import { loadProject, type ProjectLoadResult } from "../projectLoader";
import { watchFolderProject } from "./folderProjectWatcher";
import { listedProjectBaseUrl, createReaderForSource, type ProjectSource } from "./projectSource";
import { pollListedProject, type FileFingerprint } from "./listedProjectPoller";
import { createRecordingProjectFileReader } from "./recordingProjectFileReader";
import { createReloadDebouncer } from "./reloadDebouncer";

export type ProjectEngineState = {
  engine: Engine | null;
  result: ProjectLoadResult | null;
  headerNotice: string | null;
  /** Отказ `init()`/`Engine.create` — тот же текст, что в этом случае показывает страница игры. */
  engineError: string | null;
};

function formatEngineBootError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Заводит движок один раз на открытый проект («Редактор», требование 21 — без клавиатуры, мыши и
 * `tick`), грузит его тремя заходами (требование 11) и переслушивает изменения файлов проекта
 * (требования 32–35): повторный `load()` идёт в тот же движок, а не создаёт новый — движок сам
 * заменяет прежнюю игру целиком (требование 20).
 */
export function useProjectEngine(canvasRef: RefObject<HTMLCanvasElement | null>, source: ProjectSource): ProjectEngineState {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [result, setResult] = useState<ProjectLoadResult | null>(null);
  const [headerNotice, setHeaderNotice] = useState<string | null>(null);
  const [engineError, setEngineError] = useState<string | null>(null);

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
    // Перезагрузка, идущая (или только что запущенная) прямо сейчас — движок и AudioContext
    // закрываются только после того, как она сама доиграет до конца: `reload()` вызывает
    // `read_entry`/`read_texts`/`load` через несколько `await`, и закрыть их раньше значило бы
    // позвать эти методы уже на освобождённом движке (необработанное исключение wasm-bindgen).
    let activeReload: Promise<void> = Promise.resolve();

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
      // Только у проекта из списка: опрос ходит ровно по путям, которые прочитала последняя
      // загрузка («Редактор», требование 32) — у папки с диска слежение даёт `FileSystemObserver`.
      const recordingReader = source.kind === "listed" ? createRecordingProjectFileReader(baseReader) : null;
      const reader = recordingReader?.reader ?? baseReader;
      const audioContext = new AudioContext();
      createdAudioContext = audioContext;

      async function reload(): Promise<void> {
        recordingReader?.reset();
        const gameJsonText = await reader.readText("game.json");
        if (cancelled) return;
        if (gameJsonText === null) {
          setResult({ status: "entry-missing" });
        } else {
          const loadResult = await loadProject(engineInstance, reader, gameJsonText, () => audioContext);
          if (cancelled) return;
          // «Редактор», требование 16: собирает мир из сцены заново только на успешной загрузке —
          // без неё `show_scene` было бы нечего собирать, движок сам ничего не делает.
          if (loadResult.status === "ok") engineInstance.show_scene();
          setResult(loadResult);
        }
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

      function runReload(): Promise<void> {
        const promise = reload();
        activeReload = promise;
        return promise;
      }

      const debouncer = createReloadDebouncer(runReload);
      disposeDebouncer = debouncer.dispose;
      // Слежение за папкой с диска подключается сразу, ещё до конца первой загрузки («Редактор»,
      // требование 33) — изменение, сохранённое во время неё, не теряется: дребезг сам запускает
      // ещё одну перезагрузку следом за идущей.
      disposeWatch = source.kind === "folder" ? watchFolderProject(source.handle, debouncer.notify, setHeaderNotice) : () => {};

      await debouncer.runNow();
    }

    void boot();

    return () => {
      cancelled = true;
      disposeWatch?.();
      disposePoll?.();
      disposeDebouncer?.();
      void activeReload
        .catch(() => {})
        .finally(() => {
          createdEngine?.free();
          createdAudioContext?.close().catch(() => {});
        });
    };
  }, [canvasRef, source]);

  return { engine, result, headerNotice, engineError };
}
