import type { Engine } from "engine";
import { useEffect, useMemo, useRef, useState } from "react";
import type { EngineError } from "../engineErrors";
import type { LoadedMusicVerdict, LoadedSound } from "../projectLoader";
import { parseTickResult, type RawTickResult, type TickResult } from "../tickResult";
import type { SoundPlayer } from "../sound/soundPlayer";
import { createSoundPlayer } from "../sound/soundPlayer";
import { readSoundWindow } from "../sound/soundWindow";
import { buildBattleSoundAssets } from "./battleSound";
import { isPauseResumeShortcut, isPlayStopShortcut, isStepShortcut, type ShortcutKeyEvent } from "./battleShortcuts";
import { buildLiveObjectSummaries, buildLivePropertiesView, findWorldObject, resolveCanStartReplay, resolveLiveSelection, type LiveSelection } from "./battleSelection";
import type { EngineAddObjectResult, EngineEditResult, SessionMessage, StepReport, WorldObjectSummary } from "./battleTypes";
import { createLiveEditHistory, isLiveEditTargetAlive, popLiveEdit, pushLiveEdit, type LiveEditHistory } from "./liveEditHistory";
import type { ObjectPropertiesView, SceneObjectSummary } from "./sceneObjects";
import type { ProjectSource } from "./projectSource";
import { saveRecordingToProject } from "./replayFileWriter";

export type SessionMode = "edit" | "battle" | "replay";

export type BattleSessionState = {
  mode: SessionMode;
  isRunning: boolean;
  currentStep: number;
  recordingLength: number;
  stepReport: StepReport | undefined;
  messages: SessionMessage[];
  codeError: EngineError | null;
  isMuted: boolean;
  hasRecording: boolean;
  fileChangeNotice: boolean;
  saveNotice: string | null;
  openReplayError: string | null;

  liveSelection: LiveSelection | null;
  liveObjectSummaries: SceneObjectSummary[];
  livePropertiesView: ObjectPropertiesView;
  liveSelectedSummary: SceneObjectSummary | null;
  canLiveEdit: boolean;
  canUndoLiveEdit: boolean;
  /** «Редактор», требование 13: мир существует прямо сейчас — список пуст, но с разными подписями. */
  hasWorld: boolean;
  /** «Редактор», требование 12: почему «Шаг» сейчас ничего не сделает — `undefined`, когда сделает. */
  stepBlockedReason: string | undefined;

  canPlay: boolean;
  canStartReplay: boolean;

  play(): void;
  stop(): void;
  pauseOrResume(): void;
  step(): void;
  startReplay(): void;
  openReplayFile(text: string): void;
  seek(step: number): void;
  stepBack(): void;
  toggleMute(): void;
  saveRecording(): Promise<void>;
  handleShortcut(event: ShortcutKeyEvent): boolean;

  setLiveSelectedId(id: number | null): void;
  setLiveProperty(id: number, key: string, value: unknown): string | undefined;
  removeLiveProperty(id: number, key: string): void;
  copyLiveObject(id: number): void;
  deleteLiveObject(id: number): void;
  previewLiveMove(id: number, position: readonly [number, number]): void;
  commitLiveMove(id: number, position: readonly [number, number], previousPosition: readonly [number, number]): void;
  undoLiveEdit(): void;
};

type UseBattleSessionParams = {
  engine: Engine | null;
  /** `wasm.memory` из того же `init()`, что завёл движок — для чтения окна звука («Звук» → «Окно чисел»). */
  memory: WebAssembly.Memory | null;
  source: ProjectSource;
  /** Проект загружен без ошибок — «Редактор», требование 2. */
  sceneAvailable: boolean;
  loadedSounds: LoadedSound[];
  musicTracks: LoadedMusicVerdict[];
  audioContext: AudioContext | null;
  audioElement: HTMLAudioElement | null;
  /** Число объектов текущей сцены файла — клэмп выбора при «Запуске» и «Стопе». */
  sceneObjectCount: number;
  /** Выбор в режиме правки — семя живого выбора на «Запуске» (требование 15) и куда он возвращается на «Стопе» (требование 7). */
  editSelectedIndex: number | null;
  onEditSelectionChange: (index: number | null) => void;
  /** Открыт ли гейт перезагрузки после внешней правки — «Редактор», требование 37. */
  hasQueuedReload: boolean;
  setReloadGateOpen: (isOpen: boolean) => void;
};

type Snapshot = {
  worldObjects: WorldObjectSummary[];
  selection: LiveSelection | null;
  properties: Record<string, unknown> | undefined;
  stepReport: StepReport | undefined;
  messages: SessionMessage[];
  currentStep: number;
  recordingLength: number;
  hasWorld: boolean;
  stepBlockedReason: string | undefined;
};

const EMPTY_PROPERTIES_VIEW: ObjectPropertiesView = { status: "none" };
const EMPTY_SNAPSHOT: Snapshot = {
  worldObjects: [],
  selection: null,
  properties: undefined,
  stepReport: undefined,
  messages: [],
  currentStep: 0,
  recordingLength: 0,
  hasWorld: false,
  stepBlockedReason: "Нет партии",
};

/** Живой список и свойства читаются с движка заново — «Редактор», требования 8, 13–14, 23–26, 44–45. */
function readSnapshot(engine: Engine, selection: LiveSelection | null): Snapshot {
  const worldObjects = engine.world_objects() as WorldObjectSummary[];
  const resolvedSelection = resolveLiveSelection(selection, worldObjects);
  const properties = resolvedSelection ? (engine.object_properties(resolvedSelection.id) as Record<string, unknown> | undefined) : undefined;
  return {
    worldObjects,
    selection: resolvedSelection,
    properties,
    stepReport: engine.step_report() as StepReport | undefined,
    messages: engine.session_messages() as SessionMessage[],
    currentStep: engine.current_step(),
    recordingLength: engine.recording_length(),
    hasWorld: engine.has_world(),
    stepBlockedReason: engine.step_blocked() as string | undefined,
  };
}

/**
 * Настоящее брошенное исключение из вызова движка (сбой самого wasm) — не ошибка кода игры: она
 * приходит в штатном ответе `step()`/`tick()` (`{running:false, error}`) и разбирается `parseTickResult`.
 */
function toEngineError(error: unknown): EngineError {
  return { file: "движок", path: "", message: error instanceof Error ? error.message : String(error), line: null, column: null };
}

/**
 * Режимы редактора «партия», «пауза» и «повтор» — «Редактор», требования 1–48: держит текущий режим,
 * ведёт кадровый цикл партии (`tick`) и повтора (`step`) со звуком теми же модулями `web/src/sound`,
 * что у страницы игры, живой список и свойства из движка, правки на ходу с отменой и запись/повтор.
 */
export function useBattleSession(params: UseBattleSessionParams): BattleSessionState {
  const [mode, setMode] = useState<SessionMode>("edit");
  const [isRunning, setIsRunning] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const [hasRecording, setHasRecording] = useState(false);
  const [codeError, setCodeError] = useState<EngineError | null>(null);
  const [saveNotice, setSaveNotice] = useState<string | null>(null);
  const [openReplayError, setOpenReplayError] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot>(EMPTY_SNAPSHOT);
  const [liveHistory, setLiveHistory] = useState<LiveEditHistory>(createLiveEditHistory());

  const paramsRef = useRef(params);
  paramsRef.current = params;
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const codeErrorRef = useRef(codeError);
  codeErrorRef.current = codeError;
  const isMutedRef = useRef(isMuted);
  isMutedRef.current = isMuted;
  const liveHistoryRef = useRef(liveHistory);
  liveHistoryRef.current = liveHistory;
  // Источник истины для выбора внутри кадрового цикла — состояние React обновляется асинхронно,
  // а следующий кадр может понадобиться раньше повторного рендера.
  const selectionRef = useRef<LiveSelection | null>(null);
  const soundPlayerRef = useRef<{ sounds: LoadedSound[]; music: LoadedMusicVerdict[]; player: SoundPlayer } | null>(null);
  const startedAtRef = useRef<Date>(new Date());

  function applySnapshot(next: Snapshot): void {
    selectionRef.current = next.selection;
    setSnapshot(next);
  }

  function refreshSnapshot(engine: Engine): void {
    applySnapshot(readSnapshot(engine, selectionRef.current));
  }

  function ensureSoundPlayer(): SoundPlayer | null {
    const { audioContext, audioElement, loadedSounds, musicTracks } = paramsRef.current;
    if (audioContext === null || audioElement === null) return null;
    const cached = soundPlayerRef.current;
    if (cached !== null && cached.sounds === loadedSounds && cached.music === musicTracks) return cached.player;
    // Пересоздание — редко (только настоящая перезагрузка проекта между партиями, не каждая
    // правка): во время партии перезагрузка сама отложена гейтом («Редактор», требование 37).
    void buildBattleSoundAssets(audioContext, loadedSounds, musicTracks).then((assets) => {
      if (paramsRef.current.audioContext !== audioContext || paramsRef.current.audioElement !== audioElement) return;
      soundPlayerRef.current = { sounds: loadedSounds, music: musicTracks, player: createSoundPlayer(audioContext, audioElement, assets) };
    });
    return cached?.player ?? null;
  }

  function playSoundForFrame(engine: Engine, memory: WebAssembly.Memory): void {
    if (isMutedRef.current) return;
    const player = ensureSoundPlayer();
    if (player === null) return;
    player.handleFrame(readSoundWindow(memory, engine.sound_window_ptr(), engine.sound_window_len()));
  }

  /**
   * «Стоп», пауза и ошибка кода — «Редактор», требования 7, 10: звук должен стихнуть сразу, а не
   * ждать кадра, которого больше не будет (кадровый цикл как раз перестаёт вызывать `handleFrame`
   * в этот момент). То же самое «остановить трек сейчас», которым уже пользуется «Без звука».
   */
  function stopSound(): void {
    soundPlayerRef.current?.player.handleTabHidden();
  }

  // Кадровый цикл партии и повтора — «Редактор», требования 28, 39–40, 47: оба режима шагают через
  // `tick` с одним и тем же накопителем реального времени движка — в повторе он сам, не быстрее и не
  // медленнее партии, прогоняет `step_once` по записи (не больше 5 шагов за вызов, не дальше её конца).
  useEffect(() => {
    if (mode === "edit" || !isRunning) return;
    const engine = params.engine;
    if (engine === null) return;
    let frameHandle = 0;
    let stopped = false;

    function frame(now: number): void {
      if (stopped) return;
      const memory = paramsRef.current.memory;
      const result = parseTickResult(engine!.tick(now) as RawTickResult);
      if (result.status === "error") {
        setIsRunning(false);
        setCodeError(result.error);
        stopSound();
        refreshSnapshot(engine!);
        return;
      }
      if (memory) playSoundForFrame(engine!, memory);
      refreshSnapshot(engine!);
      // «Редактор», требование 29: повтор дошёл до конца записи — пауза на последнем шаге, тем же
      // способом, что и ручная пауза повтора (`pauseOrResume`): накопитель сбрасывается, чтобы
      // случайное продолжение позже не досчитывало прошедшее время одним рывком (требование 10).
      if (modeRef.current === "replay" && engine!.current_step() >= engine!.recording_length()) {
        engine!.pause();
        stopSound();
        setIsRunning(false);
        return;
      }
      frameHandle = requestAnimationFrame(frame);
    }
    frameHandle = requestAnimationFrame(frame);
    return () => {
      stopped = true;
      cancelAnimationFrame(frameHandle);
    };
  }, [mode, isRunning, params.engine]);

  function seedLiveSelection(engine: Engine): void {
    const worldObjects = engine.world_objects() as WorldObjectSummary[];
    const seedId = params.editSelectedIndex;
    const found = seedId !== null ? findWorldObject(worldObjects, seedId) : undefined;
    applySnapshot({
      worldObjects,
      selection: found ? { id: found.id, generation: found.generation } : null,
      properties: found ? (engine.object_properties(found.id) as Record<string, unknown> | undefined) : undefined,
      stepReport: undefined,
      messages: [],
      currentStep: engine.current_step(),
      recordingLength: engine.recording_length(),
      hasWorld: engine.has_world(),
      stepBlockedReason: engine.step_blocked() as string | undefined,
    });
  }

  function play(): void {
    const engine = params.engine;
    if (engine === null || !params.sceneAvailable) return;
    engine.play();
    startedAtRef.current = new Date();
    setMode("battle");
    setIsRunning(true);
    setCodeError(null);
    setHasRecording(true);
    setSaveNotice(null);
    setOpenReplayError(null);
    setLiveHistory(createLiveEditHistory());
    params.setReloadGateOpen(false);
    seedLiveSelection(engine);
  }

  function stop(): void {
    const engine = params.engine;
    if (engine === null) return;
    engine.stop();
    stopSound();
    const lastSelectionId = selectionRef.current?.id ?? null;
    setMode("edit");
    setIsRunning(false);
    setCodeError(null);
    setLiveHistory(createLiveEditHistory());
    applySnapshot(EMPTY_SNAPSHOT);
    params.onEditSelectionChange(lastSelectionId !== null && lastSelectionId < params.sceneObjectCount ? lastSelectionId : null);
    params.setReloadGateOpen(true);
  }

  function pauseOrResume(): void {
    const engine = params.engine;
    if (engine === null || mode === "edit") return;
    if (isRunning) {
      // `pause()` сам знает про повтор — там он не отпускает клавиши мира (не расходится с
      // записью), только сбрасывает накопитель реального времени (требование 10), тот же самый
      // вызов, что и в партии.
      engine.pause();
      stopSound();
      setIsRunning(false);
    } else {
      if (codeError !== null) return;
      setIsRunning(true);
    }
  }

  /** «Шаг», Ctrl+Alt+P — «Редактор», требования 9, 12, 33, 40: `step_blocked()` — то же основание,
   *  на котором кнопка и сочетание неактивны (`stepBlockedReason` снимка), проверено заново здесь на
   *  случай клика раньше следующего кадра. */
  function step(): void {
    const engine = params.engine;
    if (engine === null || engine.step_blocked() !== undefined) return;
    if (isRunning) {
      // «Редактор», требование 10: «Шаг» на ходу ставит паузу — звук должен стихнуть тем же
      // способом, что и обычная пауза (`pauseOrResume`), а не продолжать играть поверх остановленного мира.
      engine.pause();
      stopSound();
      setIsRunning(false);
    }
    let result: TickResult;
    try {
      result = parseTickResult(engine.step() as RawTickResult);
    } catch (error) {
      result = { status: "error", error: toEngineError(error) };
    }
    if (result.status === "error") setCodeError(result.error);
    refreshSnapshot(engine);
  }

  function startReplayFromText(text: string, pauseAfterStart: boolean, startedAt: Date): void {
    const engine = params.engine;
    if (engine === null) return;
    const result = engine.replay(text) as EngineEditResult;
    if (!result.ok) {
      setOpenReplayError(result.error);
      return;
    }
    startedAtRef.current = startedAt;
    setMode("replay");
    setIsRunning(!pauseAfterStart);
    setCodeError(null);
    setHasRecording(true);
    setSaveNotice(null);
    setOpenReplayError(null);
    setLiveHistory(createLiveEditHistory());
    params.setReloadGateOpen(false);
    seedLiveSelection(engine);
  }

  /**
   * «Повтор» — «Редактор», требование 34: сохраняет время НАЧАЛА партии, не время нажатия кнопки —
   * `startedAtRef` уже держит его с `play()` (или с предыдущего «Открыть запись»/«Повтор»), поэтому
   * файл сохранённой записи называется так же, как назывался бы, сохрани её сразу после «Стопа».
   */
  function startReplay(): void {
    const engine = params.engine;
    if (engine === null || !hasRecording) return;
    const text = engine.recording() as string | undefined;
    if (text === undefined) return;
    startReplayFromText(text, false, startedAtRef.current);
  }

  /** «Открыть запись» — «Редактор», требования 34–35: повтор начинается на паузе на шаге 0, имя
   *  файла при сохранении — от времени открытия (начало исходной партии файл не хранит). */
  function openReplayFile(text: string): void {
    startReplayFromText(text, true, new Date());
  }

  /**
   * Шкала и «Шаг назад» пересчитывают партию с начала и сами не трогают ни звук, ни накопитель
   * реального времени — «Редактор», требования 9, 10, 33: сюда встаёт пауза, поэтому звук стихает и
   * накопитель сбрасывается тем же `pause()`, что и везде, а шаг, на который встали, может оказаться
   * шагом ошибки — `parseTickResult` решает, оставлять ли `codeError`, а не сбрасывать его вслепую.
   */
  function afterSeek(engine: Engine, raw: unknown): void {
    const result = parseTickResult(raw as RawTickResult);
    engine.pause();
    stopSound();
    setIsRunning(false);
    setCodeError(result.status === "error" ? result.error : null);
    refreshSnapshot(engine);
  }

  function seek(step: number): void {
    const engine = params.engine;
    if (engine === null || mode !== "replay") return;
    afterSeek(engine, engine.seek(step));
  }

  function stepBack(): void {
    const engine = params.engine;
    if (engine === null || mode !== "replay") return;
    afterSeek(engine, engine.step_back());
  }

  function toggleMute(): void {
    setIsMuted((current) => {
      const next = !current;
      if (next) soundPlayerRef.current?.player.handleTabHidden();
      return next;
    });
  }

  async function saveRecording(): Promise<void> {
    const engine = params.engine;
    if (engine === null || !hasRecording) return;
    const text = engine.recording() as string | undefined;
    if (text === undefined) return;
    const result = await saveRecordingToProject(params.source, startedAtRef.current, text);
    setSaveNotice(result.ok ? `Запись сохранена: replays/${result.fileName}` : `Не сохранено — ${result.reason}`);
  }

  const canLiveEdit = mode === "battle";

  function withEngine<T>(action: (engine: Engine) => T): T | undefined {
    const engine = params.engine;
    if (engine === null || !canLiveEdit) return undefined;
    return action(engine);
  }

  function setLiveSelectedId(id: number | null): void {
    const engine = params.engine;
    if (engine === null) return;
    if (id === null) {
      applySnapshot({ ...snapshot, selection: null, properties: undefined });
      return;
    }
    const found = findWorldObject(snapshot.worldObjects, id);
    if (found === undefined) return;
    applySnapshot({ ...snapshot, selection: { id: found.id, generation: found.generation }, properties: engine.object_properties(id) as Record<string, unknown> | undefined });
  }

  function setLiveProperty(id: number, key: string, value: unknown): string | undefined {
    return withEngine((engine) => {
      const before = engine.object_properties(id) as Record<string, unknown> | undefined;
      const hadKey = before !== undefined && key in before;
      const result = engine.set_property(id, key, value) as EngineEditResult;
      if (!result.ok) return result.error;
      // Метка жизни — свежая, с этого самого момента, не унаследованная откуда-то раньше: запись
      // отмены должна знать, что это правда тот же объект, который правило может успеть подменить
      // до Ctrl+Z (крайний случай требования 21).
      const generation = findWorldObject(engine.world_objects() as WorldObjectSummary[], id)?.generation;
      if (generation !== undefined) setLiveHistory(pushLiveEdit(liveHistoryRef.current, { kind: "set", id, generation, key, hadKey, previous: hadKey ? before?.[key] : undefined }));
      refreshSnapshot(engine);
      return undefined;
    });
  }

  function removeLiveProperty(id: number, key: string): void {
    withEngine((engine) => {
      const before = engine.object_properties(id) as Record<string, unknown> | undefined;
      if (before === undefined || !(key in before)) return;
      const result = engine.remove_property(id, key) as EngineEditResult;
      if (!result.ok) return;
      const generation = findWorldObject(engine.world_objects() as WorldObjectSummary[], id)?.generation;
      if (generation !== undefined) setLiveHistory(pushLiveEdit(liveHistoryRef.current, { kind: "remove", id, generation, key, previous: before[key] }));
      refreshSnapshot(engine);
    });
  }

  function copyLiveObject(id: number): void {
    withEngine((engine) => {
      const properties = engine.object_properties(id) as Record<string, unknown> | undefined;
      if (properties === undefined) return;
      const position = properties.position;
      const shifted = Array.isArray(position) && typeof position[0] === "number" && typeof position[1] === "number" ? { ...properties, position: [position[0] + 1, position[1]] } : properties;
      const result = engine.add_object(shifted) as EngineAddObjectResult;
      if (!result.ok) return;
      // Не `refreshSnapshot` + `setLiveSelectedId` по очереди: второй читает `snapshot.worldObjects`
      // из замыкания React-состояния, которое ещё не подхватило список, обновлённый первым вызовом
      // в этом же синхронном тике, — копия остаётся не выбранной. `readSnapshot` сразу же со свежим
      // списком и выбором на новый объект.
      const worldObjects = engine.world_objects() as WorldObjectSummary[];
      const created = findWorldObject(worldObjects, result.id);
      // Метка новой копии — своя, с этого самого создания (требование 18): следующая отмена именно
      // её и должна найти, а не какую-то другую с тем же номером.
      if (created) setLiveHistory(pushLiveEdit(liveHistoryRef.current, { kind: "add", id: created.id, generation: created.generation }));
      applySnapshot(readSnapshot(engine, created ? { id: created.id, generation: created.generation } : null));
    });
  }

  function deleteLiveObject(id: number): void {
    withEngine((engine) => {
      const properties = engine.object_properties(id) as Record<string, unknown> | undefined;
      if (properties === undefined) return;
      const result = engine.delete_object(id) as EngineEditResult;
      if (!result.ok) return;
      setLiveHistory(pushLiveEdit(liveHistoryRef.current, { kind: "delete", id, properties }));
      applySnapshot(readSnapshot(engine, null));
    });
  }

  function previewLiveMove(id: number, position: readonly [number, number]): void {
    withEngine((engine) => engine.move_object(id, position[0], position[1]));
  }

  /**
   * `previousPosition` приходит от вызывающей стороны (место на начало переноса), а не читается из
   * движка: превью переноса (`previewLiveMove`, `move_object`) уже переставило объект туда же, где
   * его застаёт отпускание, так что «текущее» свойство к этому моменту — уже новое место, не старое.
   */
  function commitLiveMove(id: number, position: readonly [number, number], previousPosition: readonly [number, number]): void {
    withEngine((engine) => {
      const result = engine.set_property(id, "position", [position[0], position[1]]) as EngineEditResult;
      if (!result.ok) return;
      const generation = findWorldObject(engine.world_objects() as WorldObjectSummary[], id)?.generation;
      if (generation !== undefined) setLiveHistory(pushLiveEdit(liveHistoryRef.current, { kind: "move", id, generation, previous: previousPosition }));
      refreshSnapshot(engine);
    });
  }

  /** Ctrl+Z во время партии — «Редактор», требование 21: откатывает последнюю правку на ходу. */
  function undoLiveEdit(): void {
    withEngine((engine) => {
      const popped = popLiveEdit(liveHistoryRef.current);
      if (popped === null) return;
      setLiveHistory(popped.rest);
      const entry = popped.entry;
      // Объект, к которому относится правка, уже не тот (правило удалило его и номер занял новый,
      // требование 21, крайний случай) — отмена снимает запись без действия, кроме возврата
      // удалённого объекта: он как раз и оживляет номер заново, `isLiveEditTargetAlive` это знает.
      if (!isLiveEditTargetAlive(entry, engine.world_objects() as WorldObjectSummary[])) return;
      switch (entry.kind) {
        case "set":
          if (entry.hadKey) engine.set_property(entry.id, entry.key, entry.previous);
          else engine.remove_property(entry.id, entry.key);
          break;
        case "remove":
          engine.set_property(entry.id, entry.key, entry.previous);
          break;
        case "add":
          engine.delete_object(entry.id);
          break;
        case "delete": {
          // Не `refreshSnapshot` + `setLiveSelectedId` по очереди — та же причина, что у `copyLiveObject`:
          // `setLiveSelectedId` искал бы новый объект в `snapshot.worldObjects` из замыкания, ещё не
          // подхватившего список, который `refreshSnapshot` только что отправил в `setSnapshot`
          // асинхронно, — возвращённый объект оставался бы не выбранным (требование 18).
          const result = engine.add_object(entry.properties) as EngineAddObjectResult;
          const worldObjects = engine.world_objects() as WorldObjectSummary[];
          const restored = result.ok ? findWorldObject(worldObjects, result.id) : undefined;
          applySnapshot(readSnapshot(engine, restored ? { id: restored.id, generation: restored.generation } : null));
          return;
        }
        case "move":
          engine.set_property(entry.id, "position", [entry.previous[0], entry.previous[1]]);
          break;
      }
      refreshSnapshot(engine);
    });
  }

  /**
   * Ctrl+P/Ctrl+Shift+P/Ctrl+Alt+P — «Редактор», требование 1: работают при любом фокусе, вызывающая
   * сторона перехватывает их раньше остальных сочетаний и раньше печати браузера. Стрелки шкалы
   * повтора сюда не входят — они, в отличие от этих трёх, уважают фокус в поле ввода (требование 29)
   * и разбираются самим вызывающим кодом (`isReplaySeekShortcut`).
   */
  function handleShortcut(event: ShortcutKeyEvent): boolean {
    if (isPlayStopShortcut(event)) {
      if (modeRef.current === "edit") play();
      else stop();
      return true;
    }
    if (isPauseResumeShortcut(event)) {
      if (modeRef.current !== "edit") pauseOrResume();
      return true;
    }
    if (isStepShortcut(event)) {
      // «Редактор», требование 12: `step()` сама не делает ничего, когда `step_blocked()` не undefined.
      step();
      return true;
    }
    return false;
  }

  const liveSelectedSummary = snapshot.selection ? (findWorldObject(snapshot.worldObjects, snapshot.selection.id) ?? null) : null;
  // Тот же массив, пока `snapshot.worldObjects` не сменился, — иначе смена выбора (та же
  // «Редактор», требование 20: перенос мышью на паузе) даёт `SceneCanvas` новый `objectsVersion` на
  // каждый рендер, и его эффект на этой зависимости сбрасывает уже начатый перенос сразу после
  // нажатия («Редактор», требование 20).
  const liveObjectSummaries = useMemo(() => buildLiveObjectSummaries(snapshot.worldObjects), [snapshot.worldObjects]);

  return {
    mode,
    isRunning,
    currentStep: snapshot.currentStep,
    recordingLength: snapshot.recordingLength,
    stepReport: snapshot.stepReport,
    messages: snapshot.messages,
    codeError,
    isMuted,
    hasRecording,
    fileChangeNotice: mode !== "edit" && params.hasQueuedReload,
    saveNotice,
    openReplayError,

    liveSelection: snapshot.selection,
    liveObjectSummaries,
    livePropertiesView: mode === "edit" ? EMPTY_PROPERTIES_VIEW : buildLivePropertiesView(snapshot.properties),
    liveSelectedSummary: liveSelectedSummary ? { index: liveSelectedSummary.id, name: liveSelectedSummary.name, color: null, image: null, isOnScene: true } : null,
    canLiveEdit,
    canUndoLiveEdit: canLiveEdit && liveHistory.length > 0,
    hasWorld: snapshot.hasWorld,
    stepBlockedReason: snapshot.stepBlockedReason,

    canPlay: mode === "edit" && params.sceneAvailable && params.engine !== null,
    canStartReplay: resolveCanStartReplay(hasRecording, mode, isRunning, params.sceneAvailable),

    play,
    stop,
    pauseOrResume,
    step,
    startReplay,
    openReplayFile,
    seek,
    stepBack,
    toggleMute,
    saveRecording,
    handleShortcut,

    setLiveSelectedId,
    setLiveProperty,
    removeLiveProperty,
    copyLiveObject,
    deleteLiveObject,
    previewLiveMove,
    commitLiveMove,
    undoLiveEdit,
  };
}
