import { reconcileMusic, type MusicPerformed } from "./musicReconciler";
import type { SoundWindowSnapshot } from "./soundWindow";

export type MusicAsset = { url: string; path: string };

export type SoundAssets = {
  /** Готовые отсчёты, по одному на `SoundId` — только звуки, которые движок принял на загрузке. */
  sounds: ReadonlyMap<number, AudioBuffer>;
  /** Blob-адрес сжатого трека, по одному на `MusicId` — только треки с приговором «ok». */
  music: ReadonlyMap<number, MusicAsset>;
};

export type SoundPlayer = {
  /** Зовётся раз в кадр, сразу после чтения окна чисел движка. */
  handleFrame(snapshot: SoundWindowSnapshot): void;
  /** «У проигрывателя один хозяин»: обработчик скрытия вкладки трогает исполненное сам, а не ждёт
   *  следующей сверки — она уже не позовёт движок, пока вкладка спрятана. */
  handleTabHidden(): void;
};

function logPlayerLine(message: string): void {
  // «У журнала два писателя» — эта строка не едет в движок и фактом для него не становится.
  console.warn(`исполнитель: ${message}`);
}

/**
 * Единственный исполнитель звука на странице. Заводит короткие звуки готовыми буферами и ведёт
 * один `<audio loop>` для музыки, каждый кадр сверяя его с окном чисел движка (`reconcileMusic`).
 * Решений о том, что должно звучать, здесь нет — они уже приняты движком; эта сторона приводит
 * проигрыватель в соответствие и помнит то, чего движку знать не положено: исполненное, сломанные
 * треки, отложенную попытку после отказа браузера.
 */
export function createSoundPlayer(
  audioContext: AudioContext,
  audioElement: HTMLAudioElement,
  assets: SoundAssets,
): SoundPlayer {
  const performed: MusicPerformed = { trackId: null, playing: false };
  const brokenTrackIds = new Set<number>();
  let awaitingRetry = false;
  let hasUserGestured = false;
  // «У каждого сбоя своя отметка «уже сказано», живёт до перезагрузки страницы» — для сломанного
  // трека эту роль уже играет `brokenTrackIds` (второй попытки на него не будет), а для отказа
  // завести звук отдельной метки на трек нет, поэтому метка одна на всю страницу.
  let hasLoggedStartFailure = false;

  // «Событие pause элемента обновляет эту память» — источник истины про «играет ли» один, и это не
  // код, который её меняет, а сам элемент; наши же вызовы `pause()`/`play()` проходят через него.
  audioElement.addEventListener("pause", () => {
    performed.playing = false;
  });

  audioElement.addEventListener("error", () => {
    const trackId = performed.trackId;
    if (trackId === null || audioElement.error?.code !== MediaError.MEDIA_ERR_DECODE) return;
    brokenTrackIds.add(trackId);
    performed.playing = false;
    const asset = assets.music.get(trackId);
    logPlayerLine(
      `${asset?.path ?? trackId} — не разжимается на ${Math.floor(audioElement.currentTime)}-й секунде`,
    );
  });

  // Кадр и жест сверяют независимо и могут позвать `attemptPlay()` для одного и того же трека, пока
  // предыдущий `play()` ещё не решился, — без охраны это заводит `play()` поверх `play()`, и браузер
  // рвёт более ранний запрос как «interrupted by a new load request», один разрыв на каждый лишний
  // вызов. `pendingPlayTrackId` не пускает второй запрос на тот же трек, пока первый не осел;
  // `playRequestId` метит сам запрос, чтобы результат уже перекрытого (сменили трек, пока играли
  // прежний) не тронул состояние и не написал в журнал чужую отмену.
  let pendingPlayTrackId: number | null = null;
  let playRequestId = 0;

  /**
   * «У проигрывателя один хозяин»: всякий, кто трогает `<audio>` мимо `attemptPlay()` (пауза по
   * сверке, скрытие вкладки, отсутствующий файл трека), обязан тем же движением поправить
   * исполненное и снять отметку «попытка в пути» — иначе поздний ответ брошенного `play()` придёт
   * уже после того, как всё улеглось, и либо повторит вызов, либо перепутает состояние.
   */
  function stopPlayback(): void {
    audioElement.pause();
    performed.playing = false;
    pendingPlayTrackId = null;
    playRequestId += 1;
  }

  function attemptPlay(): void {
    const trackId = performed.trackId;
    // «Исполнитель не долбится»: пока предыдущая попытка на этот же трек не решилась, второй
    // play() поверх неё не зовётся.
    if (trackId !== null && pendingPlayTrackId === trackId) return;

    pendingPlayTrackId = trackId;
    const requestId = ++playRequestId;
    audioElement.play().then(
      () => {
        if (requestId !== playRequestId) return;
        pendingPlayTrackId = null;
        performed.playing = true;
      },
      (error: unknown) => {
        if (requestId !== playRequestId) return;
        pendingPlayTrackId = null;
        // AbortError — это не отказ браузера играть, а наш же `pause()`/смена трека, оборвавшие
        // незавершённый play(); тот, кто это сделал, уже поправил исполненное сам через
        // `stopPlayback()` или следующий `attemptPlay()`. Отложенная попытка и строка в журнал
        // положены только на NotAllowedError — настоящий отказ браузера завести звук.
        if (!(error instanceof DOMException) || error.name !== "NotAllowedError") return;
        performed.playing = false;
        awaitingRetry = true;
        // До первого нажатия игрока отказ — обычное поведение браузера, а не сбой; строка в
        // журнал идёт только после того, как звук уже разрешали, и только один раз за сессию.
        if (hasUserGestured && !hasLoggedStartFailure) {
          hasLoggedStartFailure = true;
          logPlayerLine(`не удалось завести звук (${error.message})`);
        }
      },
    );
  }

  function applyMusicAction(action: ReturnType<typeof reconcileMusic>): void {
    switch (action.kind) {
      case "none":
        return;
      case "pause":
        stopPlayback();
        return;
      case "resume":
        attemptPlay();
        return;
      case "start": {
        const asset = assets.music.get(action.trackId);
        if (!asset) {
          // Файла для нужного трека нет — заводить нечего, но и оставлять исполненное на прежнем
          // треке нельзя: следующая сверка увидела бы «то же имя» в исполненном и решила бы, что
          // он всё ещё лежит в проигрывателе, — и подняла бы старый трек через «resume».
          stopPlayback();
          audioElement.removeAttribute("src");
          performed.trackId = null;
          return;
        }
        performed.trackId = action.trackId;
        performed.playing = false;
        audioElement.src = asset.url;
        attemptPlay();
        return;
      }
    }
  }

  function playSound(id: number): void {
    const buffer = assets.sounds.get(id);
    if (!buffer) return;
    const source = audioContext.createBufferSource();
    source.buffer = buffer;
    source.connect(audioContext.destination);
    source.start();
  }

  function reconcileNow(snapshot: SoundWindowSnapshot): void {
    const action = reconcileMusic(
      { enabled: snapshot.enabled, trackId: snapshot.musicId },
      performed,
      { brokenTrackIds, awaitingRetry },
    );
    applyMusicAction(action);
  }

  // «Первое и каждое следующее pointerdown или keydown на window, в фазе перехвата»: обработчик
  // жеста только будит устройство — резюмирует контекст и снимает отложенную попытку, — а не
  // заводит музыку сам. Музыку заводит обычная сверка кадром позже, по снимку окна ПОСЛЕ tick():
  // тот же самый щелчок может на этом круге сменить экран (например, «Играть» из меню в партию), и
  // снимок, прочитанный до нажатия, назвал бы музыку уже покинутого экрана — на долю кадра
  // зазвучал бы обрывок чужой мелодии. Для `<audio>` откладывать `play()` на кадр позже безопасно:
  // разрешение браузера держится на «sticky user activation» страницы, а не на буквальном вызове
  // внутри обработчика, — резюме `AudioContext` здесь и так покрывает самый строгий случай.
  function handleGesture(): void {
    hasUserGestured = true;
    if (audioContext.state !== "running") void audioContext.resume();
    awaitingRetry = false;
  }
  window.addEventListener("pointerdown", handleGesture, true);
  window.addEventListener("keydown", handleGesture, true);

  return {
    handleFrame(snapshot) {
      // «Звук выключен: новые звуки не запускаются» — и до первого нажатия игрока запускать
      // нечего: контекст стоит в `suspended`, пока его не разбудит `handleGesture`.
      if (snapshot.enabled && audioContext.state === "running") {
        for (const id of snapshot.raisedSoundIds) playSound(id);
      }
      reconcileNow(snapshot);
    },
    handleTabHidden() {
      stopPlayback();
    },
  };
}
