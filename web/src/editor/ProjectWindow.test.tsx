// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  return { SceneCanvas: ({ wind, onWindChange }: { wind: readonly [number, number]; onWindChange: (wind: readonly [number, number]) => string | undefined }) => <WindFields wind={wind} onWindChange={onWindChange} /> };
});

const SCENE_TEXT = '{ "wind": [1.5, 0], "objects": [] }';
const GAME_JSON_TEXT = '{ "name": "Тест", "scene": { "width": 100, "height": 50 } }';

function createSceneEditing(engine: FakeBattleEngine, setSceneWind: SceneEditingState["setSceneWind"]): SceneEditingState {
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
    addProperty: noop,
    declareProperty: noop,
    copyObject: noop,
    addObject: noop,
    deleteObject: noop,
    whenIdle: () => Promise.resolve(),
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
