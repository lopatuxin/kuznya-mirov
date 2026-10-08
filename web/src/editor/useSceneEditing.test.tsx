// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectLoadResult } from "../projectLoader";
import { createEditorCameraStore, createFlatCameraStore } from "./editorCamera";
import { writeProjectFile } from "./projectFileWriter";
import type { ProjectEngineState } from "./useProjectEngine";
import { useSceneEditing, type SceneEditingState } from "./useSceneEditing";

type FullReload = (result: ProjectLoadResult, getCachedText: (relativePath: string) => string | null | undefined, writeCountAtStart: number) => void;

const projectEngineMock = vi.hoisted(() => ({ state: null as unknown as ProjectEngineState, onFullReload: null as unknown as FullReload }));

vi.mock("./useProjectEngine", () => ({
  useProjectEngine: (_canvasRef: unknown, _source: unknown, onFullReload: FullReload) => {
    projectEngineMock.onFullReload = onFullReload;
    return projectEngineMock.state;
  },
}));
vi.mock("./projectFileWriter", () => ({ writeProjectFile: vi.fn(() => Promise.resolve({ ok: true })) }));

const SOURCE = { kind: "listed", name: "qa-wind" } as const;
const GAME_JSON_TEXT = '{ "name": "Тест", "files": { "scene": "scene.json", "properties": "properties.json" } }';
const SCENE_TEXT = '{\r\n  "wind": [1.5, 0],\r\n  "objects": []\r\n}';
const SCENE_TEXT_WITHOUT_WIND = '{\r\n  "objects": []\r\n}';
const OK_RESULT = { status: "ok", warnings: [], gameJsonText: GAME_JSON_TEXT, sceneText: SCENE_TEXT, coverMasks: [] } as unknown as ProjectLoadResult;

function createProjectEngineState(setWind: (wind: unknown) => unknown): ProjectEngineState {
  return {
    engine: { set_wind: setWind } as unknown as ProjectEngineState["engine"],
    memory: null,
    editorCameraStore: createEditorCameraStore(),
    flatCameraStore: createFlatCameraStore(),
    result: null,
    loadedAt: null,
    headerNotice: null,
    engineError: null,
    hasQueuedReload: false,
    setReloadGateOpen: () => {},
    runEditedLoad: vi.fn(() => Promise.resolve(OK_RESULT)),
    getCachedText: (relativePath) => (relativePath === "properties.json" ? "{}" : undefined),
    setCachedText: () => {},
    setCachedBytes: () => {},
    getWriteCount: () => 0,
    isFilePresent: () => Promise.resolve(false),
  };
}

async function openProject(sceneText: string, setWind: (wind: unknown) => unknown): Promise<{ current: SceneEditingState }> {
  projectEngineMock.state = createProjectEngineState(setWind);
  const { result } = renderHook(() => useSceneEditing({ current: null }, SOURCE));
  await act(async () => {
    projectEngineMock.onFullReload({ ...OK_RESULT, sceneText } as ProjectLoadResult, projectEngineMock.state.getCachedText, 0);
    await result.current.whenIdle();
  });
  return result;
}

async function settle(editing: { current: SceneEditingState }): Promise<void> {
  await act(async () => {
    await editing.current.whenIdle();
  });
}

describe("useSceneEditing: setSceneWind", () => {
  beforeEach(() => {
    vi.mocked(writeProjectFile).mockClear();
  });

  afterEach(cleanup);

  it("ошибка проверки движка — возвращается её текст, файл не пишется и не проверяется загрузкой", async () => {
    const setWind = vi.fn(() => ({ ok: false, error: "wind есть только в плоской сцене" }));
    const editing = await openProject(SCENE_TEXT, setWind);

    let error: string | undefined;
    act(() => {
      error = editing.current.setSceneWind([3, 1]);
    });
    await settle(editing);

    expect(error).toBe("wind есть только в плоской сцене");
    expect(setWind).toHaveBeenCalledWith([3, 1]);
    expect(editing.current.sceneText).toBe(SCENE_TEXT);
    expect(editing.current.canUndo).toBe(false);
    expect(projectEngineMock.state.runEditedLoad).not.toHaveBeenCalled();
    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("без ошибки wind заменяется в scene.json на месте, остальной текст байт в байт", async () => {
    const setWind = vi.fn(() => ({ ok: true }));
    const editing = await openProject(SCENE_TEXT, setWind);
    const expectedText = SCENE_TEXT.replace("[1.5, 0]", "[-2, 0.5]");

    let error: string | undefined;
    act(() => {
      error = editing.current.setSceneWind([-2, 0.5]);
    });
    await settle(editing);

    expect(error).toBeUndefined();
    expect(editing.current.sceneText).toBe(expectedText);
    expect(writeProjectFile).toHaveBeenCalledTimes(1);
    expect(writeProjectFile).toHaveBeenCalledWith(SOURCE, "scene.json", expectedText);
    expect(editing.current.saveState).toEqual({ status: "saved" });
  });

  it("ключа wind не было — дописывается последним ключом корня", async () => {
    const editing = await openProject(SCENE_TEXT_WITHOUT_WIND, () => ({ ok: true }));

    act(() => editing.current.setSceneWind([0, 0]));
    await settle(editing);

    expect(writeProjectFile).toHaveBeenCalledTimes(1);
    const writtenText = vi.mocked(writeProjectFile).mock.calls[0]?.[2] as string;
    expect(Object.keys(JSON.parse(writtenText)).at(-1)).toBe("wind");
    expect(JSON.parse(writtenText).wind).toEqual([0, 0]);
  });

  it("отмена откатывает запись wind", async () => {
    const editing = await openProject(SCENE_TEXT, () => ({ ok: true }));
    act(() => editing.current.setSceneWind([4, 0]));
    await settle(editing);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.undo());
    await settle(editing);

    expect(editing.current.sceneText).toBe(SCENE_TEXT);
    expect(writeProjectFile).toHaveBeenCalledWith(SOURCE, "scene.json", SCENE_TEXT);
  });
});

const PARTICLE_SCENE_TEXT = '{ "objects": [{ "position": [1, 1], "size": [1, 1], "sparks": 0.5, "sparks_reach": 2, "sparks_spread": 45, "smoke": 0.5 }] }';

function writtenTexts(): Record<string, string> {
  return Object.fromEntries(vi.mocked(writeProjectFile).mock.calls.map(([, path, text]) => [path, text as string]));
}

describe("useSceneEditing: свойства частиц объекта", () => {
  beforeEach(() => {
    vi.mocked(writeProjectFile).mockClear();
  });

  afterEach(cleanup);

  it("«Убрать» снимает главное свойство и настройки эффекта одной записью scene.json, остальные свойства остаются", async () => {
    const editing = await openProject(PARTICLE_SCENE_TEXT, () => ({ ok: true }));

    act(() => editing.current.removeProperties(0, ["sparks", "sparks_reach", "sparks_spread"]));
    await settle(editing);

    expect(writeProjectFile).toHaveBeenCalledTimes(1);
    expect(JSON.parse(writtenTexts()["scene.json"] as string).objects[0]).toEqual({ position: [1, 1], size: [1, 1], smoke: 0.5 });
  });

  it("«Убрать» — один шаг отмены: она возвращает все свойства разом", async () => {
    const editing = await openProject(PARTICLE_SCENE_TEXT, () => ({ ok: true }));
    act(() => editing.current.removeProperties(0, ["sparks", "sparks_reach", "sparks_spread"]));
    await settle(editing);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.undo());
    await settle(editing);

    expect(editing.current.sceneText).toBe(PARTICLE_SCENE_TEXT);
    expect(editing.current.canUndo).toBe(false);
    expect(writtenTexts()).toEqual({ "scene.json": PARTICLE_SCENE_TEXT });
  });

  it("первая правка настройки, которой у объекта нет, дописывает свойство; запись одна", async () => {
    const editing = await openProject(PARTICLE_SCENE_TEXT, () => ({ ok: true }));

    act(() => editing.current.setPropertyValue(0, "smoke_height", 6));
    await settle(editing);

    expect(writeProjectFile).toHaveBeenCalledTimes(1);
    expect(JSON.parse(writtenTexts()["scene.json"] as string).objects[0].smoke_height).toBe(6);
  });

  it("проверка нашла ошибку — файл не пишется, правка остаётся на экране", async () => {
    const editing = await openProject(PARTICLE_SCENE_TEXT, () => ({ ok: true }));
    vi.mocked(projectEngineMock.state.runEditedLoad as NonNullable<ProjectEngineState["runEditedLoad"]>).mockResolvedValueOnce({
      status: "rejected",
      errors: [],
      warnings: [],
      gameJsonText: GAME_JSON_TEXT,
      sceneText: PARTICLE_SCENE_TEXT,
    });

    act(() => editing.current.setPropertyValue(0, "smoke", 2));
    await settle(editing);

    expect(writeProjectFile).not.toHaveBeenCalled();
    expect(editing.current.saveState.status).toBe("unsaved");
  });
});
