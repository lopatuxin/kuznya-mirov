import init, { Engine } from "engine";
import { computeCanvasLayout } from "./canvasLayout";
import { formatError, formatErrorScreen, type EngineError } from "./engineErrors";
import { fetchText, loadGameOptions, type GameOptionsResult } from "./gameOptions";
import { resolveGameName } from "./gameSelection";
import { parseSceneConfig, type SceneConfig } from "./sceneConfig";
import { buildWarningsBadge, shouldBlurAfterToggleActivation, type WarningsBadge } from "./warningsPanel";

type ReadEntryResult =
  | { ok: true; files: { properties: string; scene: string; rules: string }; warnings: EngineError[] }
  | { ok: false; errors: EngineError[]; warnings: EngineError[] };

type LoadResult =
  | { ok: true; warnings: EngineError[] }
  | { ok: false; errors: EngineError[]; warnings: EngineError[] };

const MESSAGE_POLL_FRAMES = 60;

const canvas = document.querySelector<HTMLCanvasElement>("#scene");
const errorBox = document.querySelector<HTMLElement>("#error");
const gameSelect = document.querySelector<HTMLElement>("#game-select");
const gameSelectList = document.querySelector<HTMLElement>("#game-select-list");
const warningsPanel = document.querySelector<HTMLElement>("#warnings-panel");
const warningsToggle = document.querySelector<HTMLButtonElement>("#warnings-toggle");
const warningsClose = document.querySelector<HTMLButtonElement>("#warnings-close");
const warningsList = document.querySelector<HTMLElement>("#warnings-list");

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

function attachVisibility(engine: Engine): void {
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) engine.tab_hidden();
  });
}

/**
 * Вписывает холст в окно, сохраняя квадратную клетку сцены (см. `computeCanvasLayout`), и при
 * наличии движка сообщает ему новый размер буфера. `engine` отсутствует при самом первом вызове —
 * до того как GPU-поверхность вообще создана на этом холсте.
 */
function applyCanvasLayout(target: HTMLCanvasElement, scene: SceneConfig, engine: Engine | null): void {
  const layout = computeCanvasLayout(
    scene.width,
    scene.height,
    window.innerWidth,
    window.innerHeight,
    window.devicePixelRatio || 1,
  );
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
function attachResize(engine: Engine, target: HTMLCanvasElement, scene: SceneConfig): void {
  let pending = false;
  window.addEventListener("resize", () => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      applyCanvasLayout(target, scene, engine);
    });
  });
}

function runLoop(engine: Engine): void {
  let loggedMessages = 0;
  let frame = 0;

  function tick(now: number): void {
    try {
      engine.tick(now);
    } catch (error) {
      // Потеря контекста видеокарты и прочие сбои отрисовки уже обработаны внутри движка; эта
      // защита — на случай непредвиденного исключения, чтобы кадровый цикл страницы не остановился.
      console.error("шаг движка завершился с ошибкой:", error);
    }

    frame += 1;
    if (frame % MESSAGE_POLL_FRAMES === 0) {
      const messages = engine.messages() as string[];
      for (const message of messages.slice(loggedMessages)) console.warn(message);
      loggedMessages = messages.length;
    }

    requestAnimationFrame(tick);
  }

  requestAnimationFrame(tick);
}

async function runGame(gameName: string): Promise<void> {
  if (!canvas || !errorBox) return;

  const baseUrl = `/games/${gameName}/`;

  const gameJsonText = await fetchText(`${baseUrl}game.json`);
  if (gameJsonText === null) {
    showError(`Не удалось получить game.json игры «${gameName}» по адресу ${baseUrl}game.json.`);
    return;
  }

  const scene = parseSceneConfig(gameJsonText);
  applyCanvasLayout(canvas, scene, null);

  await init();

  let engine: Engine;
  try {
    engine = await Engine.create(canvas);
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
    return;
  }

  const entryResult = engine.read_entry(gameJsonText) as ReadEntryResult;
  if (!entryResult.ok) {
    logWarnings(entryResult.warnings);
    showError(formatErrorScreen(entryResult.errors, entryResult.warnings));
    return;
  }

  const [propertiesText, sceneText, rulesText] = await Promise.all([
    fetchText(`${baseUrl}${entryResult.files.properties}`),
    fetchText(`${baseUrl}${entryResult.files.scene}`),
    fetchText(`${baseUrl}${entryResult.files.rules}`),
  ]);

  const loadResult = engine.load(propertiesText, sceneText, rulesText) as LoadResult;
  // read_entry (game.json) первым, load (остальные три файла) вторым — тот же порядок, в котором
  // предупреждения собирает сам движок при объединённой загрузке (см. `load_game_from_texts`).
  const warnings = [...entryResult.warnings, ...loadResult.warnings];
  logWarnings(warnings);
  if (!loadResult.ok) {
    showError(formatErrorScreen(loadResult.errors, warnings));
    return;
  }

  const badge = buildWarningsBadge(warnings);
  if (badge) showWarningsBadge(badge);

  attachInput(engine);
  attachVisibility(engine);
  attachResize(engine, canvas, scene);
  applyCanvasLayout(canvas, scene, engine);
  runLoop(engine);
}

async function boot(): Promise<void> {
  const resolution = resolveGameName(location.search);
  switch (resolution.status) {
    case "absent":
      await showGameSelection();
      return;
    case "invalid":
      showError(
        `Параметр game=«${resolution.value}» недопустим — разрешены только латинские буквы, цифры, «_» и «-». Открой страницу без параметра, чтобы увидеть список игр.`,
      );
      return;
    case "valid":
      await runGame(resolution.name);
      return;
  }
}

void boot();
