import init, { Engine } from "engine";
import { computeCanvasLayout } from "./canvasLayout";
import { formatError, formatErrorScreen, type EngineError } from "./engineErrors";
import { loadGameOptions, type GameOptionsResult } from "./gameOptions";
import { gameListSearch, resolveGameName } from "./gameSelection";
import { createHttpProjectFileReader, loadProject, type LoadedMusicVerdict, type LoadedSound } from "./projectLoader";
import { decodeSoundBuffer } from "./sound/soundLoader";
import { createSoundPlayer, type MusicAsset, type SoundPlayer } from "./sound/soundPlayer";
import { readSoundWindow } from "./sound/soundWindow";
import { parseTickResult, type RawTickResult, type TickResult } from "./tickResult";
import { buildWarningsBadge, shouldBlurAfterToggleActivation, type WarningsBadge } from "./warningsPanel";

const MESSAGE_POLL_FRAMES = 60;

const canvas = document.querySelector<HTMLCanvasElement>("#scene");
const errorBox = document.querySelector<HTMLElement>("#error");
const gameSelect = document.querySelector<HTMLElement>("#game-select");
const gameSelectList = document.querySelector<HTMLElement>("#game-select-list");
const warningsPanel = document.querySelector<HTMLElement>("#warnings-panel");
const warningsToggle = document.querySelector<HTMLButtonElement>("#warnings-toggle");
const warningsClose = document.querySelector<HTMLButtonElement>("#warnings-close");
const warningsList = document.querySelector<HTMLElement>("#warnings-list");
const backToGames = document.querySelector<HTMLButtonElement>("#back-to-games");
const musicElement = document.querySelector<HTMLAudioElement>("#music");

/**
 * Единственный выход из открытой игры. Показывается на игре и на экране ошибки, но не на самом
 * списке — там возвращаться некуда.
 */
function showBackToGames(): void {
  if (!backToGames) return;
  backToGames.hidden = false;
  backToGames.addEventListener("click", () => {
    location.href = `${location.pathname}${gameListSearch(location.search)}`;
  });
}

function showError(text: string): void {
  if (canvas) canvas.hidden = true;
  if (gameSelect) gameSelect.hidden = true;
  if (errorBox) {
    errorBox.hidden = false;
    errorBox.textContent = text;
  }
}

async function showGameSelection(): Promise<void> {
  if (!gameSelect || !gameSelectList) return;
  if (canvas) canvas.hidden = true;

  let result: GameOptionsResult;
  try {
    result = await loadGameOptions();
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
    return;
  }

  if (result.options.length === 0) {
    showError(result.failures.map((failure) => `${failure.id}: ${failure.reason}`).join("\n"));
    return;
  }

  gameSelect.hidden = false;

  for (const option of result.options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "game-option";
    button.textContent = option.name;
    button.addEventListener("click", () => {
      location.search = `?game=${encodeURIComponent(option.id)}`;
    });
    gameSelectList.append(button);
  }

  if (result.failures.length > 0) {
    const failuresLine = document.createElement("p");
    failuresLine.id = "game-select-failures";
    failuresLine.textContent = `Не запустились: ${result.failures
      .map((failure) => `${failure.id} (${failure.reason})`)
      .join("; ")}`;
    gameSelect.append(failuresLine);
  }
}

function logWarnings(warnings: EngineError[]): void {
  for (const warning of warnings) console.warn(formatError(warning));
}

/**
 * Разжимает каждый принятый движком звук в готовый `AudioBuffer` до старта кадрового цикла —
 * «Звук» → «Два вида звука». Расхождение между движком (принял WAV) и браузером
 * (не смог его разжать) сюда всё же заведено одной строкой в журнал — «У журнала два писателя»:
 * движок о таком расхождении не знает и сказать о нём не может, значит это делает исполнитель;
 * сам звук при этом просто остаётся недоступным для проигрывания, а не роняет загрузку игры.
 */
async function decodeSoundAssets(
  audioContext: AudioContext,
  sounds: LoadedSound[],
): Promise<Map<number, AudioBuffer>> {
  const buffers = new Map<number, AudioBuffer>();
  await Promise.all(
    sounds.map(async (sound) => {
      if (!sound.bytes) return;
      try {
        buffers.set(sound.index, await decodeSoundBuffer(audioContext, sound.bytes));
      } catch {
        console.warn(`исполнитель: ${sound.path} — не разжимается`);
      }
    }),
  );
  return buffers;
}

/**
 * Blob-адреса треков, годных к проигрыванию (приговор «ok»), — только для страницы игры: у неё
 * одной звук вообще играет, поэтому только она их и строит («Редактор» получает лишь приговоры,
 * без байтов, и не создаёт ни одного Blob-адреса — см. `projectLoader.ts`).
 */
function buildMusicAssets(tracks: LoadedMusicVerdict[]): Map<number, MusicAsset> {
  const assets = new Map<number, MusicAsset>();
  for (const track of tracks) {
    if (track.verdict === "ok" && track.bytes) {
      const url = URL.createObjectURL(new Blob([track.bytes as BlobPart], { type: "audio/mpeg" }));
      assets.set(track.index, { url, path: track.path });
    }
  }
  return assets;
}

/**
 * Плашка предупреждений над идущей игрой: свёрнута — виден только счётчик, разворачивается по
 * клику в список, закрывается насовсем (без способа открыть её снова, кроме перезагрузки
 * страницы) — предупреждение не повод загораживать игру дольше, чем автору нужно его прочитать.
 */
function showWarningsBadge(badge: WarningsBadge): void {
  if (!warningsPanel || !warningsToggle || !warningsClose || !warningsList) return;

  warningsToggle.textContent = badge.label;
  warningsList.textContent = badge.details;
  warningsPanel.hidden = false;

  warningsToggle.addEventListener("click", (event) => {
    warningsList.hidden = !warningsList.hidden;
    // Клик мышью оставляет кнопку в фокусе; без этого следующий же Space/Enter во время игры снова
    // дёргает эту кнопку (нативная активация фокусированной кнопки с клавиатуры) одновременно с
    // тем же нажатием, что ловит `attachInput` на `window` для самой игры. Кнопка остаётся
    // доступной с клавиатуры — снятие фокуса происходит уже после активации, а не вместо неё, и
    // только для мыши: с клавиатуры фокус остаётся, иначе следующий Tab начинает обход страницы
    // заново, а свернуть список тем же Enter уже нельзя.
    if (shouldBlurAfterToggleActivation(event)) warningsToggle.blur();
  });
  warningsClose.addEventListener("click", () => {
    warningsPanel.hidden = true;
  });
}

function attachInput(engine: Engine): void {
  window.addEventListener("keydown", (event) => engine.key_down(event.code));
  window.addEventListener("keyup", (event) => engine.key_up(event.code));
}

/**
 * `clientX`/`clientY` — те же оконные CSS-пиксели, что и `anchor`/`offset`/`size` в `screens.json`:
 * холст растянут на всё окно и стоит в его левом верхнем углу, поэтому координата курсора относительно
 * окна совпадает с координатой относительно холста.
 */
function attachMouse(engine: Engine): void {
  window.addEventListener("mousemove", (event) => engine.mouse_move(event.clientX, event.clientY));
  window.addEventListener("mousedown", () => engine.mouse_down());
  window.addEventListener("mouseup", () => engine.mouse_up());
}

/**
 * «У проигрывателя один хозяин»: скрытие вкладки останавливает музыку в обход сверки — раз в
 * фоне движок не зовут, сверка попросту не пройдёт, — поэтому обработчик тут же поправляет
 * исполненное сам, а не оставляет его лгать до следующего вызова.
 */
function attachVisibility(engine: Engine, soundPlayer: SoundPlayer): void {
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      engine.tab_hidden();
      soundPlayer.handleTabHidden();
    }
  });
}

/**
 * `matchMedia("(resolution: …dppx)")` совпадает ровно с текущим значением `devicePixelRatio`, а
 * значит перестаёт совпадать, как только оно меняется, — и `change` стреляет один раз. Слушатель
 * пересоздаёт запрос под новое значение при каждом срабатывании, иначе после первого же изменения
 * плотности экрана движок перестал бы узнавать о следующих.
 */
function attachPixelRatio(engine: Engine): void {
  const notify = (): void => engine.set_pixel_ratio(window.devicePixelRatio || 1);
  let media = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
  const onChange = (): void => {
    notify();
    media.removeEventListener("change", onChange);
    media = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    media.addEventListener("change", onChange);
  };
  media.addEventListener("change", onChange);
  notify();
}

/**
 * Растягивает холст на всё окно и при наличии движка сообщает ему новый размер буфера — сцену
 * внутрь этого буфера вписывает уже сам движок (см. `computeCanvasLayout`). `engine` отсутствует при
 * самом первом вызове — до того как GPU-поверхность вообще создана на этом холсте.
 */
function applyCanvasLayout(target: HTMLCanvasElement, engine: Engine | null): void {
  const layout = computeCanvasLayout(window.innerWidth, window.innerHeight, window.devicePixelRatio || 1);
  target.style.width = `${layout.cssWidth}px`;
  target.style.height = `${layout.cssHeight}px`;
  target.width = layout.bufferWidth;
  target.height = layout.bufferHeight;
  engine?.resize(layout.bufferWidth, layout.bufferHeight);
}

/**
 * Перетаскивание края окна шлёт `resize` многократно за кадр; без отсрочки каждое событие тут же
 * переставляло бы `canvas.width`/`height` и перенастраивало поверхность wgpu. `pending` схлопывает
 * события внутри одного кадра в один вызов `applyCanvasLayout`.
 */
function attachResize(engine: Engine, target: HTMLCanvasElement): void {
  let pending = false;
  window.addEventListener("resize", () => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      applyCanvasLayout(target, engine);
    });
  });
}

function runLoop(engine: Engine, memory: WebAssembly.Memory, soundPlayer: SoundPlayer): void {
  let loggedMessages = 0;
  let frame = 0;

  function drainMessages(): void {
    const messages = engine.messages() as string[];
    // «У журнала два писателя»: движок и исполнитель звука пишут в одну и ту же консоль, и по
    // строке должно быть видно, чья она, — см. `soundPlayer`'s «исполнитель:».
    for (const message of messages.slice(loggedMessages)) console.warn(`движок: ${message}`);
    loggedMessages = messages.length;
  }

  /**
   * «Код игры»: ошибка кода останавливает партию на месте. Страница забирает `messages` в
   * последний раз (последние `print` уже случившегося шага), показывает ошибку тем же форматом,
   * что ошибки загрузки, и глушит звук — дальше только перезагрузка страницы, кадровый цикл
   * больше не планирует следующий кадр.
   */
  function stopOnCodeError(error: EngineError): void {
    drainMessages();
    soundPlayer.handleTabHidden();
    showError(formatError(error));
  }

  function tick(now: number): void {
    let result: TickResult = { status: "running" };
    try {
      result = parseTickResult(engine.tick(now) as RawTickResult);
      if (result.status !== "error") {
        // «Звук» → «Один вызов движка»: страница читает окно после того,
        // как вызов вернул ей управление, — то есть прямо здесь, а не отдельным пересечением границы.
        const snapshot = readSoundWindow(memory, engine.sound_window_ptr(), engine.sound_window_len());
        soundPlayer.handleFrame(snapshot);
      }
    } catch (error) {
      // Потеря контекста видеокарты и прочие сбои отрисовки уже обработаны внутри движка; эта
      // защита — на случай непредвиденного исключения, чтобы кадровый цикл страницы не остановился.
      console.error("шаг движка завершился с ошибкой:", error);
    }

    if (result.status === "error") {
      stopOnCodeError(result.error);
      return;
    }

    frame += 1;
    if (frame % MESSAGE_POLL_FRAMES === 0) drainMessages();

    requestAnimationFrame(tick);
  }

  requestAnimationFrame(tick);
}

async function runGame(gameName: string): Promise<void> {
  if (!canvas || !errorBox || !musicElement) return;

  const baseUrl = `/games/${gameName}/`;
  const reader = createHttpProjectFileReader(baseUrl);

  const gameJsonText = await reader.readText("game.json");
  if (gameJsonText === null) {
    showError(`Не удалось получить game.json игры «${gameName}» по адресу ${baseUrl}game.json.`);
    return;
  }

  applyCanvasLayout(canvas, null);

  const wasm = await init();

  let engine: Engine;
  try {
    engine = await Engine.create(canvas);
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
    return;
  }

  // Создаётся внутри `loadProject`, сразу после `read_texts` — стоит в `suspended` до первого
  // нажатия игрока («Звук» → «Проигрывание на странице»), но нужен уже там: им же проверяются
  // треки из `files.music`.
  const result = await loadProject(engine, reader, gameJsonText, () => new AudioContext());
  logWarnings(result.warnings);
  if (result.status === "rejected") {
    showError(formatErrorScreen(result.errors, result.warnings));
    return;
  }

  const badge = buildWarningsBadge(result.warnings);
  if (badge) showWarningsBadge(badge);

  const soundBuffers = await decodeSoundAssets(result.audioContext, result.loadedSounds);
  const soundPlayer = createSoundPlayer(result.audioContext, musicElement, {
    sounds: soundBuffers,
    music: buildMusicAssets(result.musicTracks),
  });

  attachInput(engine);
  attachMouse(engine);
  attachVisibility(engine, soundPlayer);
  attachPixelRatio(engine);
  attachResize(engine, canvas);
  applyCanvasLayout(canvas, engine);
  runLoop(engine, wasm.memory, soundPlayer);
}

async function boot(): Promise<void> {
  const resolution = resolveGameName(location.search);
  switch (resolution.status) {
    case "absent":
      await showGameSelection();
      return;
    case "invalid":
      showBackToGames();
      showError(
        `Параметр game=«${resolution.value}» недопустим — разрешены только латинские буквы, цифры, «_» и «-». Кнопка «← К играм» внизу слева вернёт к списку.`,
      );
      return;
    case "valid":
      showBackToGames();
      await runGame(resolution.name);
      return;
  }
}

void boot();
