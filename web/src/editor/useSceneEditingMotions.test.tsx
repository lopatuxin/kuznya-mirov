// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectLoadResult } from "../projectLoader";
import { createEditorCameraStore, createFlatCameraStore } from "./editorCamera";
import { motionsTextWithSetting, readMotionNames } from "./motionsFile";
import { writeProjectFile } from "./projectFileWriter";
import type { EditedTexts, ProjectEngineState } from "./useProjectEngine";
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

const SOURCE = { kind: "listed", name: "qa-stand" } as const;
const GAME_JSON_TEXT = '{ "name": "Тест", "files": { "scene": "scene.json", "properties": "properties.json" } }';
const SCENE_TEXT = '{ "objects": [] }';
const OK_RESULT = { status: "ok", warnings: [], gameJsonText: GAME_JSON_TEXT, sceneText: SCENE_TEXT, coverMasks: [] } as unknown as ProjectLoadResult;

function createProjectEngineState(): ProjectEngineState {
  return {
    engine: null,
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

async function openProject(): Promise<{ current: SceneEditingState }> {
  projectEngineMock.state = createProjectEngineState();
  const { result } = renderHook(() => useSceneEditing({ current: null }, SOURCE));
  await act(async () => {
    projectEngineMock.onFullReload(OK_RESULT, projectEngineMock.state.getCachedText, 0);
    await result.current.whenIdle();
  });
  return result;
}

async function settle(editing: { current: SceneEditingState }): Promise<void> {
  await act(async () => {
    await editing.current.whenIdle();
  });
}

function writes(): [string, string][] {
  return vi.mocked(writeProjectFile).mock.calls.map(([, path, text]) => [path, text as string]);
}

describe("useSceneEditing: движения «Анимации»", () => {
  beforeEach(() => {
    vi.mocked(writeProjectFile).mockClear();
  });

  afterEach(cleanup);

  it("новое движение в игре без motions.json пишет файл, потом game.json с files.motions, и проверяется загрузкой с обоими", async () => {
    const editing = await openProject();

    act(() => editing.current.createMotion("hero_jog", "hero", "Jog"));
    await settle(editing);

    expect(writes().map(([path]) => path)).toEqual(["motions.json", "game.json"]);
    const [, motionsText] = writes()[0] as [string, string];
    const [, gameJsonText] = writes()[1] as [string, string];
    expect(readMotionNames(motionsText)).toEqual(["hero_jog"]);
    expect(JSON.parse(gameJsonText).files.motions).toBe("motions.json");
    const edited = vi.mocked(projectEngineMock.state.runEditedLoad as (edited: EditedTexts) => Promise<ProjectLoadResult>).mock.calls[0]?.[0];
    expect(edited?.motionsText).toBe(motionsText);
    expect(edited?.gameJsonText).toBe(gameJsonText);
    expect(editing.current.motionsText).toBe(motionsText);
    expect(editing.current.saveState).toEqual({ status: "saved" });
  });

  it("занятое имя ничего не пишет", async () => {
    const editing = await openProject();
    act(() => editing.current.createMotion("hero_jog", "hero", "Jog"));
    await settle(editing);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.createMotion("hero_jog", "hero", "Sprint"));
    await settle(editing);

    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("правка ползунка пишет motions.json одной записью, а «Отменить» возвращает прежний текст", async () => {
    const editing = await openProject();
    act(() => editing.current.createMotion("hero_jog", "hero", "Jog"));
    await settle(editing);
    const created = editing.current.motionsText;
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.editMotions((text) => motionsTextWithSetting(text, "hero_jog", "lean", 12)));
    await settle(editing);

    expect(writes().map(([path]) => path)).toEqual(["motions.json"]);
    expect(JSON.parse(editing.current.motionsText ?? "").hero_jog.lean).toBe(12);
    expect(editing.current.canUndo).toBe(true);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.undo());
    await settle(editing);

    expect(editing.current.motionsText).toBe(created);
    expect(writes()).toEqual([["motions.json", created as string]]);
  });

  it("отмена нового движения оставляет в файле пустую таблицу", async () => {
    const editing = await openProject();
    act(() => editing.current.createMotion("hero_jog", "hero", "Jog"));
    await settle(editing);

    act(() => editing.current.undo());
    await settle(editing);

    expect(editing.current.motionsText).toBe("{}\n");
    expect(editing.current.canUndo).toBe(false);
  });

  it("правка, что ничего не меняет, не пишет файлов и не кладёт шаг в историю", async () => {
    const editing = await openProject();
    act(() => editing.current.createMotion("hero_jog", "hero", "Jog"));
    await settle(editing);
    act(() => editing.current.editMotions((text) => motionsTextWithSetting(text, "hero_jog", "speed", 2)));
    await settle(editing);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.editMotions((text) => motionsTextWithSetting(text, "hero_jog", "speed", 2)));
    await settle(editing);

    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("motions.json, что поменяли снаружи, показывается и ложится в историю", async () => {
    const editing = await openProject();
    const external = '{ "run": { "model": "hero", "clip": "Jog" } }';
    const named = '{ "name": "Тест", "files": { "scene": "scene.json", "properties": "properties.json", "motions": "motions.json" } }';

    await act(async () => {
      projectEngineMock.onFullReload(
        { ...OK_RESULT, gameJsonText: named } as ProjectLoadResult,
        (relativePath) => (relativePath === "properties.json" ? "{}" : relativePath === "motions.json" ? external : undefined),
        0,
      );
      await editing.current.whenIdle();
    });

    expect(editing.current.motionsText).toBe(external);
    expect(readMotionNames(editing.current.motionsText)).toEqual(["run"]);
  });
});
