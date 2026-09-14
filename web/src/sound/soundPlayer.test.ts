import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSoundPlayer, type SoundAssets } from "./soundPlayer";
import type { SoundWindowSnapshot } from "./soundWindow";

// «Кадр и жест сверяют независимо» — воспроизводит гонку из бага: несколько тиков кадра и клик
// подряд, пока первый `play()` ещё не решился, не должны заводить второй `play()` поверх первого и
// не должны писать в журнал больше одной строки на настоящий сбой.

type PlayCall = { resolve: () => void; reject: (error: unknown) => void };

function makeFakeAudioElement() {
  const pauseListeners: Array<() => void> = [];
  const calls: PlayCall[] = [];
  const element = {
    src: "",
    currentTime: 0,
    error: null,
    addEventListener(type: string, handler: () => void) {
      if (type === "pause") pauseListeners.push(handler);
    },
    play(): Promise<void> {
      return new Promise((resolve, reject) => {
        calls.push({ resolve, reject });
      });
    },
    pause() {
      pauseListeners.forEach((listener) => listener());
    },
    removeAttribute(name: string) {
      if (name === "src") element.src = "";
    },
  };
  return { element: element as unknown as HTMLAudioElement, calls };
}

function makeGestureCapture() {
  const handlers: Array<() => void> = [];
  const fakeWindow = {
    addEventListener(_type: string, handler: () => void) {
      handlers.push(handler);
    },
  };
  vi.stubGlobal("window", fakeWindow);
  return () => handlers.forEach((handler) => handler());
}

const FAKE_AUDIO_CONTEXT = { state: "running", resume: async () => {} } as unknown as AudioContext;

function snapshot(musicId: number | null): SoundWindowSnapshot {
  return { enabled: true, musicId, raisedSoundIds: [] };
}

describe("createSoundPlayer — гонка play()/резюме одного трека", () => {
  let warn: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("повторные кадры и клик подряд до ответа play() не заводят второй play() на тот же трек", () => {
    const fireGesture = makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[1, { url: "blob:track1", path: "menu.mp3" }]]),
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    player.handleFrame(snapshot(1)); // «start» — единственный настоящий play()
    player.handleFrame(snapshot(1)); // тот же трек, play() ещё не решился — должно быть «resume»-но-но-оп
    player.handleFrame(snapshot(1));
    fireGesture(); // клик в ту же гонку — тоже должен промолчать

    expect(calls).toHaveLength(1);
    expect(warn).not.toHaveBeenCalled();
  });

  it("резолв play() после гонки даёт нормальную работу дальше, без лишних вызовов", async () => {
    makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[1, { url: "blob:track1", path: "menu.mp3" }]]),
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    player.handleFrame(snapshot(1));
    player.handleFrame(snapshot(1));
    calls[0].resolve();
    await Promise.resolve();
    await Promise.resolve();

    player.handleFrame(snapshot(1)); // уже играет — «none»
    expect(calls).toHaveLength(1);
    expect(warn).not.toHaveBeenCalled();
  });

  it("смена трека, пока прежний play() ещё висит, не путает состояние и не логирует чужую отмену", async () => {
    makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([
        [1, { url: "blob:track1", path: "menu.mp3" }],
        [2, { url: "blob:track2", path: "game.mp3" }],
      ]),
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    player.handleFrame(snapshot(1)); // play() #1 повисает
    player.handleFrame(snapshot(2)); // другой трек — новый play() #2, #1 более не актуален
    expect(calls).toHaveLength(2);

    // Браузер рвёт более ранний запрос как AbortError — это не сбой игрока, а следствие смены
    // трека, и строка в журнал за него идти не должна.
    calls[0].reject(new Error("The play() request was interrupted by a new load request."));
    await Promise.resolve();
    await Promise.resolve();
    expect(warn).not.toHaveBeenCalled();

    calls[1].resolve();
    await Promise.resolve();
    await Promise.resolve();

    player.handleFrame(snapshot(2)); // трек 2 уже играет — «none», третьего play() нет
    expect(calls).toHaveLength(2);
    expect(warn).not.toHaveBeenCalled();
  });
});

describe("createSoundPlayer — обработчик жеста только будит устройство", () => {
  beforeEach(() => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("клик не заводит музыку по устаревшему снимку — ждёт сверки по кадру после tick()", () => {
    const fireGesture = makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[1, { url: "blob:track1", path: "menu.mp3" }]]),
    };
    const resume = vi.fn(async () => {});
    const audioContext = { state: "suspended", resume } as unknown as AudioContext;
    const player = createSoundPlayer(audioContext, element, assets);

    player.handleFrame(snapshot(null)); // снимок экрана без музыки — сделан ДО клика
    fireGesture(); // клик: должен только разбудить контекст, не завести трек по старому снимку
    expect(calls).toHaveLength(0);
    expect(resume).toHaveBeenCalled(); // «Обработчик нажатия только будит устройство»

    // Следующий tick() уже мог сменить экран — сверка по СВЕЖЕМУ снимку и заводит музыку.
    (audioContext as unknown as { state: string }).state = "running";
    player.handleFrame(snapshot(1));
    expect(calls).toHaveLength(1);
  });
});

describe("createSoundPlayer — сбои play() после отказа браузера", () => {
  let warn: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("AbortError от собственного pause()/handleTabHidden не пишет в журнал и не ставит отложенную попытку", async () => {
    makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[1, { url: "blob:track1", path: "menu.mp3" }]]),
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    player.handleFrame(snapshot(1)); // play() #1 повисает
    player.handleTabHidden(); // «У проигрывателя один хозяин»: обрывает попытку сам
    calls[0].reject(new DOMException("The play() request was interrupted.", "AbortError"));
    await Promise.resolve();
    await Promise.resolve();

    expect(warn).not.toHaveBeenCalled();

    // Отложенной попытки быть не должно — вернулись на экран с той же музыкой, сверка заводит её
    // заново, а не ждёт следующего действия игрока.
    player.handleFrame(snapshot(1));
    expect(calls).toHaveLength(2);
  });

  it("NotAllowedError после нажатия пишет в журнал один раз, даже если отказ повторяется", async () => {
    const fireGesture = makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[1, { url: "blob:track1", path: "menu.mp3" }]]),
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    // До первого нажатия игрока отказ — обычное поведение браузера, а не сбой.
    player.handleFrame(snapshot(1)); // play() #1
    calls[0].reject(new DOMException("denied", "NotAllowedError"));
    await Promise.resolve();
    await Promise.resolve();
    expect(warn).not.toHaveBeenCalled();

    fireGesture(); // первое нажатие снимает отложенную попытку
    player.handleFrame(snapshot(1)); // play() #2
    calls[1].reject(new DOMException("denied", "NotAllowedError"));
    await Promise.resolve();
    await Promise.resolve();
    expect(warn).toHaveBeenCalledTimes(1);

    fireGesture(); // второе нажатие — попытка повторяется, но строка в журнал не повторяется
    player.handleFrame(snapshot(1)); // play() #3
    calls[2].reject(new DOMException("denied", "NotAllowedError"));
    await Promise.resolve();
    await Promise.resolve();
    expect(warn).toHaveBeenCalledTimes(1);
  });

  it("трек без файла не оживляет прежний трек — src очищается, а не остаётся прежним", () => {
    makeGestureCapture();
    const { element, calls } = makeFakeAudioElement();
    const assets: SoundAssets = {
      sounds: new Map(),
      music: new Map([[2, { url: "blob:track2", path: "game.mp3" }]]), // трек 1 без файла
    };
    const player = createSoundPlayer(FAKE_AUDIO_CONTEXT, element, assets);

    player.handleFrame(snapshot(1)); // нужен трек 1 — файла нет
    expect(calls).toHaveLength(0);
    expect(element.src).toBe("");

    player.handleFrame(snapshot(1)); // повторный кадр с тем же (отсутствующим) треком — по-прежнему тишина
    expect(calls).toHaveLength(0);

    player.handleFrame(snapshot(2)); // сменился на трек с файлом — заводится он, а не призрак трека 1
    expect(calls).toHaveLength(1);
    expect(element.src).toBe("blob:track2");
  });
});
