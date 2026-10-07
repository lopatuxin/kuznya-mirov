// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFakeBattleEngine, type FakeBattleEngine } from "./fakeBattleEngine";
import { ProjectWindow } from "./ProjectWindow";
import { createEditorCameraStore, createFlatCameraStore } from "./editorCamera";
import { NO_MASKS } from "./maskBytes";
import type { SceneEditingState } from "./useSceneEditing";

const sceneEditingMock = vi.hoisted(() => ({ current: null as unknown as SceneEditingState }));

vi.mock("./useSceneEditing", () => ({ useSceneEditing: () => sceneEditingMock.current }));
// Поля окошка «Ветер» — настоящие, остальное окно сцены (холст, мышь, ручки) этому тесту не нужно.
vi.mock("./SceneCanvas", async () => {
  const { WindFields } = await import("./WindFields");
  return {
    SceneCanvas: ({
      wind,
      onWindChange,
      onDropParticles,
    }: {
      wind: readonly [number, number];
      onWindChange: (wind: readonly [number, number]) => string | undefined;
      onDropParticles: (kindName: string, x: number, y: number) => void;
    }) => (
      <>
        <WindFields wind={wind} onWindChange={onWindChange} />
        <button type="button" onClick={() => onDropParticles("дым", 10, 20)}>
          бросить вид
        </button>
      </>
    ),
  };
});

const SCENE_TEXT = '{ "wind": [1.5, 0], "objects": [] }';
const GAME_JSON_TEXT = '{ "name": "Тест", "scene": { "width": 100, "height": 50 } }';

function createSceneEditing(engine: FakeBattleEngine, setSceneWind: SceneEditingState["setSceneWind"], overrides: Partial<SceneEditingState> = {}): SceneEditingState {
  const noop = (): void => {};
  return {
    engine,
    memory: null,
    editorCameraStore: createEditorCameraStore(),
    flatCameraStore: createFlatCameraStore(),
    result: {
      status: "ok",
      warnings: [],
      gameJsonText: GAME_JSON_TEXT,
      sceneText: SCENE_TEXT,
      loadedSounds: [],
      musicTracks: [],
      stamps: [],
      coverMasks: [],
      images: [],
      audioContext: null as unknown as AudioContext,
    },
    loadedAt: null,
    headerNotice: null,
    engineError: null,
    hasQueuedReload: false,
    setReloadGateOpen: noop,
    sceneText: SCENE_TEXT,
    propertiesText: "{}",
    terrainText: null,
    particlesText: null,
    masks: NO_MASKS,
    saveState: { status: "saved" },
    canUndo: false,
    selectedIndex: null,
    setSelectedIndex: noop,
    selectedImprintIndex: null,
    setSelectedImprintIndex: noop,
    selectedParticleName: null,
    setSelectedParticleName: noop,
    undo: noop,
    transformObject: noop,
    paintTerrain: noop,
    paintCovers: noop,
    reloadDisplayed: noop,
    setTerrainWater: noop,
    setSceneWind,
    placeImprint: noop,
    replaceImprint: noop,
    setImprintValue: noop,
    copyImprint: noop,
    deleteImprint: noop,
    setParticleValue: noop,
    addParticleSource: noop,
    copyParticleKind: noop,
    deleteParticleKind: noop,
    renameParticleKind: noop,
    setPropertyValue: noop,
    removeProperty: noop,
    addProperty: noop,
    declareProperty: noop,
    copyObject: noop,
    addObject: noop,
    deleteObject: noop,
    whenIdle: () => Promise.resolve(),
    ...overrides,
  };
}

function renderWindow(): void {
  render(<ProjectWindow source={{ kind: "listed", name: "qa-wind" }} onBackToProjects={() => {}} brushFields={{ size: 4, strength: 50, onSizeChange: () => {}, onStrengthChange: () => {} }} />);
}

function windInput(label: "По x" | "По y"): HTMLInputElement {
  return screen.getByLabelText(label);
}

function pressPlayStopShortcut(): void {
  fireEvent.keyDown(window, { code: "KeyP", key: "p", ctrlKey: true });
}

/** Набирает число в поле и оставляет его открытым — как автор, не нажавший Enter. */
function typeIntoWindInput(label: "По x" | "По y", text: string): void {
  const input = windInput(label);
  act(() => input.focus());
  fireEvent.change(input, { target: { value: text } });
}

describe("окошко «Ветер» в окне проекта: открытое поле и смена партии", () => {
  let engine: FakeBattleEngine;
  let setSceneWind: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", () => 0);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    engine = createFakeBattleEngine({ fileWind: [1.5, 0] });
    setSceneWind = vi.fn((wind: readonly [number, number]) => {
      engine.calls.push(`setSceneWind ${JSON.stringify(wind)}`);
      return undefined;
    });
    sceneEditingMock.current = createSceneEditing(engine, setSceneWind);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("вне партии поля показывают ветер файла", () => {
    renderWindow();
    expect(windInput("По x").value).toBe("1.5");
    expect(windInput("По y").value).toBe("0");
  });

  it("черновик, начатый вне партии, при Ctrl+P пишется в файл, а не ставится живой игре", async () => {
    renderWindow();
    typeIntoWindInput("По x", "3");

    pressPlayStopShortcut();

    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    expect(engine.calls).toEqual(["setSceneWind [3,0]", "play"]);
    expect(engine.setWindParticles).not.toHaveBeenCalled();
    expect(windInput("По x").value).toBe("1.5");
  });

  it("то же при щелчке по «Запуску»: уход фокуса с поля принимает набранное до начала партии", async () => {
    renderWindow();
    typeIntoWindInput("По y", "-0.5");

    act(() => screen.getByRole("button", { name: "Запуск" }).focus());
    fireEvent.click(screen.getByRole("button", { name: "Запуск" }));

    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    expect(engine.calls).toEqual(["setSceneWind [1.5,-0.5]", "play"]);
  });

  it("черновик, начатый в партии, при Ctrl+P ставится живой игре до «Стопа», а в файл не пишется", async () => {
    renderWindow();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    typeIntoWindInput("По x", "-2");

    pressPlayStopShortcut();

    expect(engine.calls).toEqual(["play", "set_wind_particles [-2,0]", "stop"]);
    expect(setSceneWind).not.toHaveBeenCalled();
    expect(windInput("По x").value).toBe("1.5");
  });

  it("то же при щелчке по «Стопу»", async () => {
    renderWindow();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    typeIntoWindInput("По y", "2");

    act(() => screen.getByRole("button", { name: "Стоп" }).focus());
    fireEvent.click(screen.getByRole("button", { name: "Стоп" }));

    expect(engine.calls).toEqual(["play", "set_wind_particles [1.5,2]", "stop"]);
    expect(setSceneWind).not.toHaveBeenCalled();
  });
});

const PARTICLES_GAME_JSON_TEXT = '{ "name": "Тест", "scene": { "width": 100, "height": 50 }, "files": { "images": { "puff": {}, "spark": {} }, "particles": "particles.json" } }';
const PARTICLES_TEXT = '{ "дым": { "image": "puff", "rate": 6, "lifetime": 2, "size": 1 }, "искры": { "image": "spark", "rate": 9, "lifetime": 1, "size": 0.2 } }';
const PARTICLES_SCENE_TEXT = '{ "wind": [1.5, 0], "objects": [{ "position": [1, 1], "size": [1, 1], "particles": "дым", "layer": 2, "parallax": 0.6 }] }';

function createParticlesSceneEditing(engine: FakeBattleEngine, overrides: Partial<SceneEditingState> = {}, gameJsonText = PARTICLES_GAME_JSON_TEXT): SceneEditingState {
  const base = createSceneEditing(engine, vi.fn());
  return createSceneEditing(engine, vi.fn(), {
    result: { ...(base.result as Extract<SceneEditingState["result"], { status: "ok" }>), gameJsonText, sceneText: PARTICLES_SCENE_TEXT },
    sceneText: PARTICLES_SCENE_TEXT,
    particlesText: PARTICLES_TEXT,
    ...overrides,
  });
}

function openParticlesTab(): void {
  fireEvent.click(screen.getByRole("tab", { name: "Частицы" }));
}

describe("вкладка «Частицы» в окне проекта", () => {
  let engine: FakeBattleEngine;

  beforeEach(() => {
    window.HTMLElement.prototype.scrollIntoView = vi.fn();
    vi.stubGlobal("requestAnimationFrame", () => 0);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    engine = createFakeBattleEngine();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("в плоской сцене вкладка есть, в трёхмерной — нет", () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine);
    renderWindow();
    expect(screen.getByRole("tab", { name: "Частицы" })).toBeTruthy();
    cleanup();

    sceneEditingMock.current = createParticlesSceneEditing(engine, {}, PARTICLES_GAME_JSON_TEXT.replace('"scene": {', '"scene": { "camera": { "position": [0, 0, 5] },'));
    renderWindow();
    expect(screen.queryByRole("tab", { name: "Частицы" })).toBeNull();
  });

  it("вне партии набор в поле ставит виды движку, Enter пишет файл через действие правки", () => {
    const setParticleValue = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setParticleValue });
    renderWindow();
    openParticlesTab();

    const input = screen.getByLabelText("тяжесть");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "2" } });

    expect(engine.calls.at(-1)).toContain("set_wind_particles particles");
    expect(JSON.parse(engine.calls.at(-1)?.replace("set_wind_particles particles ", "") ?? "{}").дым.gravity).toBe(2);
    expect(setParticleValue).not.toHaveBeenCalled();

    fireEvent.keyDown(input, { key: "Enter" });
    expect(setParticleValue).toHaveBeenCalledWith("дым", "gravity", 2);
  });

  it("в проекте без загруженной игры набор и Enter не зовут предпросмотр и не пишут «игра не загружена», файл пишется", () => {
    const setParticleValue = vi.fn();
    vi.mocked(engine.has_world).mockReturnValue(false);
    engine.setWindParticles.mockReturnValue({ ok: false, error: "игра не загружена" });
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setParticleValue });
    renderWindow();
    openParticlesTab();

    const input = screen.getByLabelText("тяжесть");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "2" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(engine.setWindParticles).not.toHaveBeenCalled();
    expect(setParticleValue).toHaveBeenCalledWith("дым", "gravity", 2);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ошибка проверки загрузкой стоит у своего поля, а у вида с ошибкой поля правятся", () => {
    const errors = [
      { file: "particles.json", path: "дым → gravity", message: "gravity: должно быть числом", line: null, column: null },
      { file: "scene.json", path: "objects → 0", message: "чужая ошибка", line: null, column: null },
    ];
    const setParticleValue = vi.fn();
    const gameJsonText = PARTICLES_GAME_JSON_TEXT.replace('"files": {', '"files": { "scene": "scene.json", "properties": "properties.json",');
    sceneEditingMock.current = {
      ...createParticlesSceneEditing(engine, { setParticleValue }, gameJsonText),
      result: { status: "rejected", errors, warnings: [], gameJsonText, sceneText: PARTICLES_SCENE_TEXT },
    };
    vi.mocked(engine.has_world).mockReturnValue(false);
    renderWindow();
    openParticlesTab();

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("gravity: должно быть числом");
    expect(alert.closest(".particles-field")?.querySelector("input")).toBe(screen.getByLabelText("тяжесть"));

    const input = screen.getByLabelText("тяжесть");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "1" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(setParticleValue).toHaveBeenCalledWith("дым", "gravity", 1);
  });

  it("в партии правка поля ставится живой игре без записи файла, а «Копировать» и «Удалить» неактивны", async () => {
    const setParticleValue = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setParticleValue });
    renderWindow();
    openParticlesTab();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());

    const input = screen.getByLabelText("рост, раз");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "3" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(setParticleValue).not.toHaveBeenCalled();
    expect(engine.calls.filter((call) => call.startsWith("set_wind_particles particles")).length).toBeGreaterThanOrEqual(2);
    expect((screen.getByLabelText("рост, раз") as HTMLInputElement).value).toBe("3");
    for (const name of ["Копировать", "Удалить"]) expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("«Стоп» возвращает виды файла в поля", async () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine);
    renderWindow();
    openParticlesTab();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    const input = screen.getByLabelText("рост, раз");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "3" } });
    fireEvent.keyDown(input, { key: "Enter" });

    pressPlayStopShortcut();

    expect((screen.getByLabelText("рост, раз") as HTMLInputElement).value).toBe("1");
  });

  it("в повторе неактивно всё", async () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine);
    renderWindow();
    openParticlesTab();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());
    pressPlayStopShortcut();
    fireEvent.click(screen.getByTitle("Повтор и записи партий"));
    fireEvent.click(screen.getByText("Повтор", { selector: ".tool-menu__item-label, span" }));

    expect((screen.getByLabelText("рост, раз") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Копировать" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getAllByRole("option")[0]?.getAttribute("draggable")).toBe("false");
  });

  it("без файла видов вкладка показывает готовые дым, искры и листья", () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine, { particlesText: null });
    renderWindow();
    openParticlesTab();

    expect(within(screen.getByRole("listbox", { name: "Виды частиц" })).getAllByRole("option").map((card) => card.textContent)).toEqual(["дым", "искры", "листья"]);
  });

  it("вид, отпущенный на объект, встаёт источником с его layer и parallax — дым на трубе идёт поверх избы", () => {
    const addParticleSource = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { addParticleSource });
    vi.mocked(engine.object_at).mockReturnValue(0);
    renderWindow();

    fireEvent.click(screen.getByRole("button", { name: "бросить вид" }));

    expect(engine.object_at).toHaveBeenCalledWith(10, 20);
    expect(engine.scene_point).toHaveBeenCalledWith(10, 20, 0.6);
    expect(addParticleSource).toHaveBeenCalledWith("дым", { position: [4.5, 2.5], size: [1, 1], particles: "дым", layer: 2, parallax: 0.6 });
  });

  it("вид, отпущенный мимо объектов, берёт layer и parallax выбранного", () => {
    const addParticleSource = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { addParticleSource, selectedIndex: 0 });
    renderWindow();

    fireEvent.click(screen.getByRole("button", { name: "бросить вид" }));

    expect(addParticleSource).toHaveBeenCalledWith("дым", { position: [4.5, 2.5], size: [1, 1], particles: "дым", layer: 2, parallax: 0.6 });
  });

  it("на паузе вид, отпущенный на сцену, становится живым объектом через add_object", async () => {
    const addObject = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { addObject });
    renderWindow();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "бросить вид" }));

    expect(addObject).not.toHaveBeenCalled();
    expect(engine.add_object).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], particles: "дым" });
  });

  it("в партии готовый вид, которого нет в файле, сначала ставится живой игре, потом встаёт источник", async () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine, { particlesText: '{ "туман": { "rate": 2, "lifetime": 3, "size": 1 } }' });
    renderWindow();
    pressPlayStopShortcut();
    await waitFor(() => expect(engine.play).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "бросить вид" }));

    const liveTable = engine.setWindParticles.mock.calls.at(-1)?.[0] as { particles?: Record<string, unknown> };
    expect(Object.keys(liveTable.particles ?? {})).toEqual(["дым", "искры", "листья", "туман"]);
    expect(engine.setWindParticles.mock.invocationCallOrder.at(-1)).toBeLessThan(vi.mocked(engine.add_object).mock.invocationCallOrder[0] as number);
    expect(engine.add_object).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], particles: "дым" });
  });
});
