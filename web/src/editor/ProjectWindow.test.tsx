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
      onDropParticles: (effectId: string, x: number, y: number) => void;
    }) => (
      <>
        <WindFields wind={wind} onWindChange={onWindChange} />
        {["smoke", "sparks", "leaves"].map((effectId) => (
          <button key={effectId} type="button" onClick={() => onDropParticles(effectId, 10, 20)}>
            бросить {effectId}
          </button>
        ))}
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
    masks: NO_MASKS,
    saveState: { status: "saved" },
    canUndo: false,
    selectedIndex: null,
    setSelectedIndex: noop,
    selectedImprintIndex: null,
    setSelectedImprintIndex: noop,
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
    setPropertyValue: noop,
    removeProperty: noop,
    removeProperties: noop,
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
    expect(engine.setWind).not.toHaveBeenCalled();
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

    expect(engine.calls).toEqual(["play", "set_wind [-2,0]", "stop"]);
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

    expect(engine.calls).toEqual(["play", "set_wind [1.5,2]", "stop"]);
    expect(setSceneWind).not.toHaveBeenCalled();
  });
});

const PARTICLES_GAME_JSON_TEXT = '{ "name": "Тест", "scene": { "width": 100, "height": 50 }, "files": { "images": { "izba": {}, "birch": {} } } }';
const IZBA = { position: [1, 1], size: [4, 3], image: "izba", layer: 2, parallax: 0.6 };
const SOURCE = { position: [6, 1], size: [1, 1], smoke: 0.5, sparks: 0.5, sparks_reach: 2, layer: 2 };
const SOURCE_RECT = { x: 5, y: 15, width: 12, height: 8 };
const PARTICLES_SCENE_TEXT =JSON.stringify({ wind: [1.5, 0], objects: [IZBA, SOURCE] });

function createParticlesSceneEditing(engine: FakeBattleEngine, overrides: Partial<SceneEditingState> = {}, gameJsonText = PARTICLES_GAME_JSON_TEXT): SceneEditingState {
  const base = createSceneEditing(engine, vi.fn());
  return createSceneEditing(engine, vi.fn(), {
    result: { ...(base.result as Extract<SceneEditingState["result"], { status: "ok" }>), gameJsonText, sceneText: PARTICLES_SCENE_TEXT },
    sceneText: PARTICLES_SCENE_TEXT,
    selectedIndex: 1,
    ...overrides,
  });
}

function openParticlesTab(): void {
  fireEvent.click(screen.getByRole("tab", { name: "Частицы" }));
}

function densitySlider(): HTMLInputElement {
  return screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement;
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

  it("выбранный источник показывает группы «Дым» и «Искры», выбранная изба без частиц — подсказку", () => {
    sceneEditingMock.current = createParticlesSceneEditing(engine);
    renderWindow();
    openParticlesTab();

    expect(screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual(["Дым", "Искры"]);
    cleanup();

    sceneEditingMock.current = createParticlesSceneEditing(engine, { selectedIndex: 0 });
    renderWindow();
    openParticlesTab();

    expect(screen.queryAllByRole("heading", { level: 3 })).toHaveLength(0);
    expect(screen.getByText(/Дым и искры перетащите туда, откуда они идут/)).toBeTruthy();
  });

  it("вне партии ползунок зовёт set_property на каждое движение, а scene.json пишется один раз — при отпускании", () => {
    const setPropertyValue = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setPropertyValue });
    renderWindow();
    openParticlesTab();

    fireEvent.input(densitySlider(), { target: { value: "0.7" } });
    fireEvent.input(densitySlider(), { target: { value: "0.9" } });

    expect(engine.set_property).toHaveBeenCalledTimes(2);
    expect(engine.set_property).toHaveBeenLastCalledWith(1, "smoke", 0.9);
    expect(setPropertyValue).not.toHaveBeenCalled();

    fireEvent.change(densitySlider());

    expect(setPropertyValue).toHaveBeenCalledTimes(1);
    expect(setPropertyValue).toHaveBeenCalledWith(1, "smoke", 0.9);
  });

  it("ползунок вернули на прежнее значение — файл не пишется", () => {
    const setPropertyValue = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setPropertyValue });
    renderWindow();
    openParticlesTab();

    fireEvent.input(densitySlider(), { target: { value: "0.8" } });
    fireEvent.input(densitySlider(), { target: { value: "0.5" } });
    fireEvent.change(densitySlider());

    expect(setPropertyValue).not.toHaveBeenCalled();
  });

  it("число, которое движок не принял, стоит в поле с текстом ошибки, файл не пишется, мир возвращён", () => {
    const setPropertyValue = vi.fn();
    vi.mocked(engine.set_property).mockReturnValueOnce({ ok: false, error: "sparks_reach должно быть больше нуля, получено 0" });
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setPropertyValue });
    renderWindow();
    openParticlesTab();
    const input = screen.getByLabelText("как далеко летят, клеток");

    act(() => input.focus());
    fireEvent.change(input, { target: { value: "0" } });
    vi.mocked(engine.set_property).mockReturnValueOnce({ ok: false, error: "sparks_reach должно быть больше нуля, получено 0" });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByRole("alert").textContent).toBe("как далеко летят, клеток должно быть больше нуля, получено 0");
    expect(setPropertyValue).not.toHaveBeenCalled();
    expect(engine.set_property).toHaveBeenLastCalledWith(1, "sparks_reach", 2);
  });

  describe("перечитывание файлов проекта держат ползунок, круг и число, пока жест идёт", () => {
    it("ползунок: первое движение закрывает перечитывание, отпускание открывает", () => {
      const setReloadGateOpen = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setReloadGateOpen });
      renderWindow();
      openParticlesTab();

      fireEvent.input(densitySlider(), { target: { value: "0.7" } });
      fireEvent.input(densitySlider(), { target: { value: "0.9" } });
      expect(setReloadGateOpen.mock.calls).toEqual([[false], [false]]);

      fireEvent.change(densitySlider());
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(true);
    });

    it("число: от первой набранной цифры до Enter", () => {
      const setReloadGateOpen = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setReloadGateOpen });
      renderWindow();
      openParticlesTab();
      const input = screen.getByLabelText("как далеко летят, клеток");

      act(() => input.focus());
      fireEvent.change(input, { target: { value: "3" } });
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(false);

      fireEvent.keyDown(input, { key: "Enter" });
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(true);
    });

    it("круг: от нажатия на ручку до отпускания", () => {
      const setReloadGateOpen = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setReloadGateOpen });
      renderWindow();
      openParticlesTab();
      const dial = screen.getByRole("group", { name: "направление и разброс искр" });

      fireEvent.pointerDown(dial.querySelector("[data-part=direction]") as Element, { pointerId: 1 });
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(false);

      fireEvent.pointerUp(dial, { pointerId: 1 });
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(true);
    });

    it("жест, брошенный пересозданием панели (смена объекта), перечитывание не оставляет закрытым", () => {
      const setReloadGateOpen = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setReloadGateOpen });
      renderWindow();
      openParticlesTab();
      fireEvent.input(densitySlider(), { target: { value: "0.7" } });
      expect(setReloadGateOpen).toHaveBeenLastCalledWith(false);

      cleanup();

      expect(setReloadGateOpen).toHaveBeenLastCalledWith(true);
    });

    it("в партии ползунок перечитывание не трогает: его держит сама партия", async () => {
      const setReloadGateOpen = vi.fn();
      vi.mocked(engine.world_objects).mockReturnValue([{ id: 1, generation: 1, name: null }]);
      vi.mocked(engine.object_properties).mockImplementation(() => ({ ...SOURCE }));
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setReloadGateOpen });
      renderWindow();
      openParticlesTab();
      pressPlayStopShortcut();
      await waitFor(() => expect(engine.play).toHaveBeenCalled());
      setReloadGateOpen.mockClear();

      fireEvent.input(densitySlider(), { target: { value: "0.7" } });
      fireEvent.change(densitySlider());

      expect(setReloadGateOpen).not.toHaveBeenCalled();
    });
  });

  it("в проекте без загруженной игры движок не зовётся, правка идёт в файл", () => {
    const setPropertyValue = vi.fn();
    vi.mocked(engine.has_world).mockReturnValue(false);
    sceneEditingMock.current = createParticlesSceneEditing(engine, { setPropertyValue });
    renderWindow();
    openParticlesTab();
    const input = screen.getByLabelText("как далеко летят, клеток");

    act(() => input.focus());
    fireEvent.change(input, { target: { value: "3" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(engine.set_property).not.toHaveBeenCalled();
    expect(setPropertyValue).toHaveBeenCalledWith(1, "sparks_reach", 3);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("«Убрать» снимает все свойства эффекта, которые есть у объекта, одним вызовом правки", () => {
    const removeProperties = vi.fn();
    sceneEditingMock.current = createParticlesSceneEditing(engine, { removeProperties });
    renderWindow();
    openParticlesTab();

    fireEvent.click(within(screen.getByRole("region", { name: "Искры" })).getByRole("button", { name: "Убрать" }));

    expect(removeProperties).toHaveBeenCalledWith(1, ["sparks", "sparks_reach"]);
  });

  it("проект со старым particles у объекта показывает ошибку движка во вкладке «Ошибки»", () => {
    const error = { file: "scene.json", path: "objects → 1", message: 'неизвестное свойство "particles": нет ни среди встроенных, ни в properties.json', line: null, column: null };
    sceneEditingMock.current = {
      ...createParticlesSceneEditing(engine),
      result: { status: "rejected", errors: [error], warnings: [], gameJsonText: PARTICLES_GAME_JSON_TEXT, sceneText: PARTICLES_SCENE_TEXT },
    };
    renderWindow();

    expect(screen.getByText(/неизвестное свойство "particles"/)).toBeTruthy();
    expect(screen.getByText("scene.json: objects → 1")).toBeTruthy();
  });

  describe("в партии", () => {
    beforeEach(() => {
      vi.mocked(engine.world_objects).mockReturnValue([
        { id: 0, generation: 1, name: null },
        { id: 1, generation: 1, name: null },
      ]);
      vi.mocked(engine.object_properties).mockImplementation((id: number) => (id === 1 ? { ...SOURCE } : { ...IZBA }));
    });

    async function startBattle(): Promise<void> {
      pressPlayStopShortcut();
      await waitFor(() => expect(engine.play).toHaveBeenCalled());
    }

    it("ползунок правит живой мир без записи файла, а Ctrl+Z возвращает значение до жеста одним шагом", async () => {
      const setPropertyValue = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { setPropertyValue });
      renderWindow();
      openParticlesTab();
      await startBattle();

      fireEvent.input(densitySlider(), { target: { value: "0.7" } });
      fireEvent.input(densitySlider(), { target: { value: "0.9" } });
      fireEvent.change(densitySlider());

      expect(setPropertyValue).not.toHaveBeenCalled();
      expect(engine.set_property).toHaveBeenLastCalledWith(1, "smoke", 0.9);

      fireEvent.keyDown(window, { code: "KeyZ", key: "z", ctrlKey: true });

      expect(engine.set_property).toHaveBeenLastCalledWith(1, "smoke", 0.5);
    });

    it("«Убрать» снимает свойства из живого мира, одна отмена возвращает их", async () => {
      sceneEditingMock.current = createParticlesSceneEditing(engine);
      renderWindow();
      openParticlesTab();
      await startBattle();

      fireEvent.click(within(screen.getByRole("region", { name: "Искры" })).getByRole("button", { name: "Убрать" }));

      expect(engine.remove_property).toHaveBeenCalledWith(1, "sparks");
      expect(engine.remove_property).toHaveBeenCalledWith(1, "sparks_reach");

      fireEvent.keyDown(window, { code: "KeyZ", key: "z", ctrlKey: true });

      expect(engine.set_property).toHaveBeenCalledWith(1, "sparks", 0.5);
      expect(engine.set_property).toHaveBeenCalledWith(1, "sparks_reach", 2);
    });

    it("в повторе поля и карточки неактивны", async () => {
      sceneEditingMock.current = createParticlesSceneEditing(engine);
      renderWindow();
      openParticlesTab();
      await startBattle();
      pressPlayStopShortcut();
      fireEvent.click(screen.getByTitle("Повтор и записи партий"));
      fireEvent.click(screen.getByText("Повтор", { selector: ".tool-menu__item-label, span" }));

      expect(densitySlider().disabled).toBe(true);
      expect(screen.getAllByRole("listitem")[0]?.getAttribute("draggable")).toBe("false");
    });
  });

  describe("карточка, отпущенная на сцену", () => {
    it("дым над источником, у которого дым уже есть, ничего не меняет", () => {
      const addProperty = vi.fn();
      const setSelectedIndex = vi.fn();
      const addObject = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, setSelectedIndex, addObject });
      vi.mocked(engine.object_at).mockReturnValue(0);
      vi.mocked(engine.object_rect).mockImplementation((id) => (id === 1 ? SOURCE_RECT : undefined));
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить smoke" }));

      expect(addProperty).not.toHaveBeenCalled();
      expect(addObject).not.toHaveBeenCalled();
      expect(setSelectedIndex).not.toHaveBeenCalled();
    });

    it("искры над источником без картинки и цвета, лежащим на избе, дописываются ему со значением 0,5", () => {
      const addProperty = vi.fn();
      const setSelectedIndex = vi.fn();
      const addObject = vi.fn();
      const sceneText = JSON.stringify({ objects: [IZBA, { position: [6, 1], size: [1, 1], smoke: 0.5 }] });
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, setSelectedIndex, addObject, sceneText });
      vi.mocked(engine.object_at).mockReturnValue(0);
      vi.mocked(engine.object_rect).mockImplementation((id) => (id === 1 ? SOURCE_RECT : undefined));
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить sparks" }));

      expect(addProperty).toHaveBeenCalledWith(1, "sparks", 0.5);
      expect(setSelectedIndex).toHaveBeenCalledWith(1);
      expect(addObject).not.toHaveBeenCalled();
    });

    it("искры мимо источника, но над избой, создают новый объект", () => {
      const addObject = vi.fn();
      const addProperty = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addObject, addProperty });
      vi.mocked(engine.object_at).mockReturnValue(0);
      vi.mocked(engine.object_rect).mockImplementation((id) => (id === 1 ? { ...SOURCE_RECT, x: 200 } : undefined));
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить sparks" }));

      expect(addObject).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], sparks: 0.5, layer: 2, parallax: 0.6 });
      expect(addProperty).not.toHaveBeenCalled();
    });

    it("дым над избой создаёт новый объект 1×1 с smoke 0,5 и слоем избы, серединой под указателем", () => {
      const addObject = vi.fn();
      const addProperty = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addObject, addProperty });
      vi.mocked(engine.object_at).mockReturnValue(0);
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить smoke" }));

      expect(engine.scene_point).toHaveBeenCalledWith(10, 20, 0.6);
      expect(addObject).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], smoke: 0.5, layer: 2, parallax: 0.6 });
      expect(addProperty).not.toHaveBeenCalled();
    });

    it("дым мимо объектов берёт слой выбранного", () => {
      const addObject = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addObject, selectedIndex: 0 });
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить smoke" }));

      expect(addObject).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], smoke: 0.5, layer: 2, parallax: 0.6 });
    });

    it("листья над берёзой дописываются ей со значением 0,3 и выбирают её; мимо объектов ничего не происходит", () => {
      const addProperty = vi.fn();
      const setSelectedIndex = vi.fn();
      const addObject = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, setSelectedIndex, addObject });
      vi.mocked(engine.object_at).mockReturnValue(0);
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить leaves" }));

      expect(addProperty).toHaveBeenCalledWith(0, "leaf_fall", 0.3);
      expect(setSelectedIndex).toHaveBeenCalledWith(0);
      expect(addObject).not.toHaveBeenCalled();

      addProperty.mockClear();
      vi.mocked(engine.object_at).mockReturnValue(undefined);
      fireEvent.click(screen.getByRole("button", { name: "бросить leaves" }));

      expect(addProperty).not.toHaveBeenCalled();
      expect(addObject).not.toHaveBeenCalled();
    });

    it("листья не достаются небу и дальним холмам с repeat_x: цель — верхний объект, которому их можно дописать", () => {
      const addProperty = vi.fn();
      const setSelectedIndex = vi.fn();
      const sky = { position: [0, 0], size: [100, 50], image: "sky", repeat_x: true, layer: 5 };
      const sceneText = JSON.stringify({ objects: [IZBA, sky] });
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, setSelectedIndex, sceneText });
      vi.mocked(engine.object_at).mockReturnValue(1);
      vi.mocked(engine.object_rect).mockReturnValue(SOURCE_RECT);
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить leaves" }));

      expect(addProperty).toHaveBeenCalledTimes(1);
      expect(addProperty).toHaveBeenCalledWith(0, "leaf_fall", 0.3);
      expect(setSelectedIndex).toHaveBeenCalledWith(0);
    });

    it("под указателем одно небо с repeat_x или объект без position и size — это мимо объектов, ничего не происходит", () => {
      const addProperty = vi.fn();
      const setSelectedIndex = vi.fn();
      const addObject = vi.fn();
      const sky = { position: [0, 0], size: [100, 50], image: "sky", repeat_x: true };
      const sceneText = JSON.stringify({ objects: [sky, { image: "birch" }] });
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, setSelectedIndex, addObject, sceneText });
      vi.mocked(engine.object_at).mockReturnValue(0);
      vi.mocked(engine.object_rect).mockReturnValue(SOURCE_RECT);
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить leaves" }));

      expect(addProperty).not.toHaveBeenCalled();
      expect(setSelectedIndex).not.toHaveBeenCalled();
      expect(addObject).not.toHaveBeenCalled();
    });

    it("дым над источником с repeat_x нового объекта не пропускает в него: дописать ему нельзя, встаёт новый", () => {
      const addProperty = vi.fn();
      const addObject = vi.fn();
      const sceneText = JSON.stringify({ objects: [IZBA, { position: [6, 1], size: [1, 1], repeat_x: true }] });
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addProperty, addObject, sceneText });
      vi.mocked(engine.object_at).mockReturnValue(0);
      vi.mocked(engine.object_rect).mockImplementation((id) => (id === 1 ? SOURCE_RECT : undefined));
      renderWindow();

      fireEvent.click(screen.getByRole("button", { name: "бросить smoke" }));

      expect(addProperty).not.toHaveBeenCalled();
      expect(addObject).toHaveBeenCalledTimes(1);
    });

    it("на паузе новый источник становится живым объектом через add_object, а существующему эффект ставится живым set_property", async () => {
      const addObject = vi.fn();
      sceneEditingMock.current = createParticlesSceneEditing(engine, { addObject, selectedIndex: 0 });
      vi.mocked(engine.world_objects).mockReturnValue([{ id: 0, generation: 1, name: null }]);
      vi.mocked(engine.object_properties).mockImplementation(() => ({ ...IZBA }));
      renderWindow();
      pressPlayStopShortcut();
      await waitFor(() => expect(engine.play).toHaveBeenCalled());

      fireEvent.click(screen.getByRole("button", { name: "бросить smoke" }));

      expect(addObject).not.toHaveBeenCalled();
      expect(engine.add_object).toHaveBeenCalledWith({ position: [4.5, 2.5], size: [1, 1], smoke: 0.5, layer: 2, parallax: 0.6 });

      vi.mocked(engine.object_at).mockReturnValue(0);
      fireEvent.click(screen.getByRole("button", { name: "бросить leaves" }));

      expect(engine.set_property).toHaveBeenCalledWith(0, "leaf_fall", 0.3);
    });
  });
});

const CLOUDS_GAME_JSON_TEXT = JSON.stringify({
  name: "Тест",
  scene: { width: 100, height: 50 },
  files: {
    images: {
      cloud_a: { path: "images/cloud_a.png", size: [6, 3] },
      cloud_b: { path: "images/cloud_b.png", size: [5, 2] },
      strip: { path: "images/strip.png", size: [2, 2], frames: 4 },
      film: { path: "images/film.mp4", size: [4, 2] },
    },
  },
});
const SKY = { position: [0, 0], size: [100, 30], image: "sky", repeat_x: true, layer: 0 };
const TREE = { position: [10, 8], size: [4, 8], image: "birch", layer: 2 };

describe("группа «Облака» в колонке «Свойства»", () => {
  let engine: FakeBattleEngine;

  function createCloudsSceneEditing(overrides: Partial<SceneEditingState> = {}, objects: unknown[] = [SKY, TREE]): SceneEditingState {
    const sceneText = JSON.stringify({ objects });
    const base = createSceneEditing(engine, vi.fn());
    return createSceneEditing(engine, vi.fn(), {
      result: { ...(base.result as Extract<SceneEditingState["result"], { status: "ok" }>), gameJsonText: CLOUDS_GAME_JSON_TEXT, sceneText },
      sceneText,
      ...overrides,
    });
  }

  function cloudsGroup(): HTMLElement {
    return screen.getByRole("region", { name: "Облака" });
  }

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

  it("у неба без облаков одна кнопка «Добавить облака»: clouds 0,3 одной записью; у объекта без repeat_x группы нет", () => {
    const setPropertyValue = vi.fn();
    sceneEditingMock.current = createCloudsSceneEditing({ setPropertyValue, selectedIndex: 0 });
    renderWindow();

    expect(within(cloudsGroup()).getAllByRole("button").map((button) => button.textContent)).toEqual(["Добавить облака"]);
    fireEvent.click(screen.getByRole("button", { name: "Добавить облака" }));

    expect(setPropertyValue).toHaveBeenCalledTimes(1);
    expect(setPropertyValue).toHaveBeenCalledWith(0, "clouds", 0.3);
    cleanup();

    sceneEditingMock.current = createCloudsSceneEditing({ selectedIndex: 1 });
    renderWindow();
    expect(screen.queryByRole("region", { name: "Облака" })).toBeNull();
  });

  it("во вкладке «Частицы» облаков нет: три карточки и нет группы", () => {
    sceneEditingMock.current = createCloudsSceneEditing({ selectedIndex: 0 }, [{ ...SKY, clouds: 0.3 }]);
    renderWindow();
    openParticlesTab();

    const particles = screen.getByRole("list", { name: "Эффекты частиц" });
    expect(within(particles).getAllByRole("listitem").map((card) => card.textContent)).toEqual(["дым", "искры", "листья"]);
    expect(screen.getAllByRole("region", { name: "Облака" })).toHaveLength(1);
  });

  it("clouds и cloud_images не показаны строками общего списка свойств, остальные свойства на месте", () => {
    sceneEditingMock.current = createCloudsSceneEditing({ selectedIndex: 0 }, [{ ...SKY, clouds: 0.3, cloud_images: ["cloud_a", "cloud_b"] }]);
    renderWindow();

    const keys = Array.from(document.querySelectorAll(".property-row__key")).map((key) => key.textContent);
    expect(keys).toContain("repeat_x");
    expect(keys).not.toContain("clouds");
    expect(keys).not.toContain("cloud_images");
    expect(within(cloudsGroup()).getByRole("list", { name: "Выбранные картинки облаков" }).textContent).toBe("cloud_acloud_b");
  });

  it("«+ картинка» предлагает только годные картинки игры, выбор пишет список одной правкой", () => {
    const setPropertyValue = vi.fn();
    sceneEditingMock.current = createCloudsSceneEditing({ setPropertyValue, selectedIndex: 0 }, [{ ...SKY, clouds: 0.3, cloud_images: ["cloud_a"] }]);
    renderWindow();

    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    expect(within(screen.getByRole("list", { name: "Картинки, годные облакам" })).getAllByRole("button").map((button) => button.textContent)).toEqual(["cloud_b"]);
    fireEvent.click(screen.getByRole("button", { name: "cloud_b" }));

    expect(setPropertyValue).toHaveBeenCalledTimes(1);
    expect(setPropertyValue).toHaveBeenCalledWith(0, "cloud_images", ["cloud_a", "cloud_b"]);
  });

  it("крестик пишет список без картинки одной правкой, «Убрать» снимает оба свойства одной правкой", () => {
    const setPropertyValue = vi.fn();
    const removeProperties = vi.fn();
    sceneEditingMock.current = createCloudsSceneEditing({ setPropertyValue, removeProperties, selectedIndex: 0 }, [{ ...SKY, clouds: 0.3, cloud_images: ["cloud_a", "cloud_b"] }]);
    renderWindow();

    fireEvent.click(screen.getByRole("button", { name: "Убрать картинку cloud_a" }));
    expect(setPropertyValue).toHaveBeenCalledTimes(1);
    expect(setPropertyValue).toHaveBeenCalledWith(0, "cloud_images", ["cloud_b"]);

    fireEvent.click(within(cloudsGroup()).getByRole("button", { name: "Убрать" }));
    expect(removeProperties).toHaveBeenCalledTimes(1);
    expect(removeProperties).toHaveBeenCalledWith(0, ["clouds", "cloud_images"]);
  });

  it("ползунок «сколько облаков» зовёт set_property на каждое движение, scene.json пишется один раз — при отпускании", () => {
    const setPropertyValue = vi.fn();
    sceneEditingMock.current = createCloudsSceneEditing({ setPropertyValue, selectedIndex: 0 }, [{ ...SKY, clouds: 0.3 }]);
    renderWindow();
    const slider = screen.getByRole("slider", { name: "сколько облаков" });

    fireEvent.input(slider, { target: { value: "0.5" } });
    fireEvent.input(slider, { target: { value: "0.75" } });
    expect(engine.set_property).toHaveBeenCalledTimes(2);
    expect(engine.set_property).toHaveBeenLastCalledWith(0, "clouds", 0.75);
    expect(setPropertyValue).not.toHaveBeenCalled();

    fireEvent.change(slider);
    expect(setPropertyValue).toHaveBeenCalledTimes(1);
    expect(setPropertyValue).toHaveBeenCalledWith(0, "clouds", 0.75);
  });

  describe("в партии", () => {
    beforeEach(() => {
      vi.mocked(engine.world_objects).mockReturnValue([
        { id: 0, generation: 1, name: null },
        { id: 1, generation: 1, name: null },
      ]);
    });

    async function startBattle(): Promise<void> {
      pressPlayStopShortcut();
      await waitFor(() => expect(engine.play).toHaveBeenCalled());
    }

    it("«Добавить облака» на паузе — живой set_property, а не запись файла", async () => {
      const setPropertyValue = vi.fn();
      vi.mocked(engine.object_properties).mockImplementation((id: number) => ({ ...[SKY, TREE][id] }));
      sceneEditingMock.current = createCloudsSceneEditing({ selectedIndex: 0, setPropertyValue });
      renderWindow();
      await startBattle();

      fireEvent.click(screen.getByRole("button", { name: "Добавить облака" }));

      expect(engine.set_property).toHaveBeenCalledWith(0, "clouds", 0.3);
      expect(setPropertyValue).not.toHaveBeenCalled();
    });

    it("добавление картинки в партии — живой set_property со всем списком, а не запись файла", async () => {
      const setPropertyValue = vi.fn();
      vi.mocked(engine.object_properties).mockImplementation((id: number) => ({ ...[{ ...SKY, clouds: 0.3, cloud_images: ["cloud_a"] }, TREE][id] }));
      sceneEditingMock.current = createCloudsSceneEditing({ selectedIndex: 0, setPropertyValue });
      renderWindow();
      await startBattle();

      fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
      fireEvent.click(screen.getByRole("button", { name: "cloud_b" }));

      expect(engine.set_property).toHaveBeenLastCalledWith(0, "cloud_images", ["cloud_a", "cloud_b"]);
      expect(setPropertyValue).not.toHaveBeenCalled();
    });
  });
});
