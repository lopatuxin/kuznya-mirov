// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectLoadResult } from "../projectLoader";
import { createEditorCameraStore, createFlatCameraStore } from "./editorCamera";
import { builtInParticleFields } from "./particlePresets";
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

function createProjectEngineState(setWindParticles: (settings: unknown) => unknown): ProjectEngineState {
  return {
    engine: { set_wind_particles: setWindParticles } as unknown as ProjectEngineState["engine"],
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

async function openProject(sceneText: string, setWindParticles: (settings: unknown) => unknown): Promise<{ current: SceneEditingState }> {
  projectEngineMock.state = createProjectEngineState(setWindParticles);
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
    const setWindParticles = vi.fn(() => ({ ok: false, error: "wind есть только в плоской сцене" }));
    const editing = await openProject(SCENE_TEXT, setWindParticles);

    let error: string | undefined;
    act(() => {
      error = editing.current.setSceneWind([3, 1]);
    });
    await settle(editing);

    expect(error).toBe("wind есть только в плоской сцене");
    expect(setWindParticles).toHaveBeenCalledWith({ wind: [3, 1] });
    expect(editing.current.sceneText).toBe(SCENE_TEXT);
    expect(editing.current.canUndo).toBe(false);
    expect(projectEngineMock.state.runEditedLoad).not.toHaveBeenCalled();
    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("без ошибки wind заменяется в scene.json на месте, остальной текст байт в байт", async () => {
    const setWindParticles = vi.fn(() => ({ ok: true }));
    const editing = await openProject(SCENE_TEXT, setWindParticles);
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

const GAME_JSON_WITH_PARTICLES = '{ "name": "Тест", "files": { "scene": "scene.json", "properties": "properties.json", "particles": "particles.json" } }';
const PARTICLES_TEXT = '{\n  "дым": { "image": "puff", "rate": 6, "lifetime": 2, "size": 1 },\n  "искры": { "image": "spark", "rate": 9, "lifetime": 1, "size": 0.2 }\n}\n';
const PARTICLE_SCENE_TEXT = '{ "objects": [{ "position": [1, 1], "size": [1, 1], "particles": "дым" }, { "position": [2, 1], "size": [1, 1], "particles": "искры" }] }';

async function openParticlesProject(options: { hasFile: boolean }): Promise<{ current: SceneEditingState }> {
  const gameJsonText = options.hasFile ? GAME_JSON_WITH_PARTICLES : GAME_JSON_TEXT;
  const okResult = { ...OK_RESULT, gameJsonText, sceneText: PARTICLE_SCENE_TEXT } as unknown as ProjectLoadResult;
  projectEngineMock.state = {
    ...createProjectEngineState(() => ({ ok: true })),
    runEditedLoad: vi.fn(() => Promise.resolve(okResult)),
    getCachedText: (relativePath) => {
      if (relativePath === "properties.json") return "{}";
      return relativePath === "particles.json" && options.hasFile ? PARTICLES_TEXT : undefined;
    },
  };
  const { result } = renderHook(() => useSceneEditing({ current: null }, SOURCE));
  await act(async () => {
    projectEngineMock.onFullReload(okResult, projectEngineMock.state.getCachedText, 0);
    await result.current.whenIdle();
  });
  return result;
}

function writtenTexts(): Record<string, string> {
  return Object.fromEntries(vi.mocked(writeProjectFile).mock.calls.map(([, path, text]) => [path, text as string]));
}

describe("useSceneEditing: вкладка «Частицы»", () => {
  beforeEach(() => {
    vi.mocked(writeProjectFile).mockClear();
  });

  afterEach(cleanup);

  it("файл видов читается в показанный текст", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    expect(editing.current.particlesText).toBe(PARTICLES_TEXT);
  });

  it("значение поля пишется в particles.json на месте, остальные файлы не пишутся", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    act(() => editing.current.setParticleValue("дым", "rate", 9));
    await settle(editing);

    expect(writtenTexts()).toEqual({ "particles.json": PARTICLES_TEXT.replace('"rate": 6', '"rate": 9') });
    expect(projectEngineMock.state.runEditedLoad).toHaveBeenCalledWith(expect.objectContaining({ particlesText: PARTICLES_TEXT.replace('"rate": 6', '"rate": 9') }));
    expect(editing.current.saveState).toEqual({ status: "saved" });
  });

  it("то же значение — ничего не пишется", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    act(() => editing.current.setParticleValue("дым", "rate", 6));
    await settle(editing);

    expect(writeProjectFile).not.toHaveBeenCalled();
    expect(editing.current.canUndo).toBe(false);
  });

  it("проверка нашла ошибку — файл не пишется, правка остаётся на экране", async () => {
    const editing = await openParticlesProject({ hasFile: true });
    vi.mocked(projectEngineMock.state.runEditedLoad as NonNullable<ProjectEngineState["runEditedLoad"]>).mockResolvedValueOnce({
      status: "rejected",
      errors: [],
      warnings: [],
      gameJsonText: GAME_JSON_WITH_PARTICLES,
      sceneText: PARTICLE_SCENE_TEXT,
    });

    act(() => editing.current.setParticleValue("дым", "rate", -1));
    await settle(editing);

    expect(writeProjectFile).not.toHaveBeenCalled();
    expect(editing.current.particlesText).toBe(PARTICLES_TEXT.replace('"rate": 6', '"rate": -1'));
    expect(editing.current.saveState.status).toBe("unsaved");
  });

  it("первая правка готового вида заводит particles.json и files.particles: файл раньше game.json, вид записан целиком", async () => {
    const editing = await openParticlesProject({ hasFile: false });

    act(() => editing.current.setParticleValue("дым", "rate", 9));
    await settle(editing);

    const calls = vi.mocked(writeProjectFile).mock.calls.map(([, path]) => path);
    expect(calls).toEqual(["particles.json", "game.json"]);
    const written = writtenTexts();
    expect(JSON.parse(written["game.json"] as string).files.particles).toBe("particles.json");
    expect(JSON.parse(written["particles.json"] as string)).toEqual({ дым: { ...builtInParticleFields("дым"), rate: 9 } });
    expect(written["particles.json"]?.endsWith("}\n")).toBe(true);
    expect(editing.current.selectedParticleName).toBe("дым");
  });

  it("отмена первой правки оставляет файл и ключ с пустой таблицей", async () => {
    const editing = await openParticlesProject({ hasFile: false });
    act(() => editing.current.setParticleValue("дым", "rate", 9));
    await settle(editing);
    vi.mocked(writeProjectFile).mockClear();

    act(() => editing.current.undo());
    await settle(editing);

    expect(editing.current.particlesText).toBe("{}\n");
    expect(writtenTexts()).toEqual({ "particles.json": "{}\n" });
  });

  it("готовый вид, перетащенный на сцену, записан в файл, источник — в scene.json и выбран, одним действием", async () => {
    const editing = await openParticlesProject({ hasFile: true });
    const objectCount = (JSON.parse(editing.current.sceneText ?? "{}") as { objects: unknown[] }).objects.length;

    act(() => editing.current.addParticleSource("листья", { position: [3, 3], size: [1, 1], particles: "листья" }));
    await settle(editing);

    expect(Object.keys(JSON.parse(editing.current.particlesText ?? "{}"))).toEqual(["дым", "искры", "листья"]);
    expect((JSON.parse(editing.current.sceneText ?? "{}") as { objects: unknown[] }).objects).toHaveLength(objectCount + 1);
    expect(editing.current.selectedIndex).toBe(objectCount);
  });

  it("«Копировать» — копия последней в файле, выбрана она", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    act(() => editing.current.copyParticleKind("искры"));
    await settle(editing);

    expect(editing.current.selectedParticleName).toBe("искры-копия");
    expect(Object.keys(JSON.parse(editing.current.particlesText ?? "{}"))).toEqual(["дым", "искры", "искры-копия"]);
  });

  it("удаление пишет оба файла: вид пропал, у источников particles снят; одна отмена откатывает оба", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    act(() => editing.current.deleteParticleKind("дым"));
    await settle(editing);

    const written = writtenTexts();
    expect(Object.keys(JSON.parse(written["particles.json"] as string))).toEqual(["искры"]);
    expect((JSON.parse(written["scene.json"] as string).objects as Record<string, unknown>[]).map((object) => object.particles)).toEqual([undefined, "искры"]);

    vi.mocked(writeProjectFile).mockClear();
    act(() => editing.current.undo());
    await settle(editing);

    expect(writtenTexts()).toEqual({ "particles.json": PARTICLES_TEXT, "scene.json": PARTICLE_SCENE_TEXT });
  });

  it("переименование пишет оба файла: вид на своём месте, у источников новое имя; одна отмена откатывает оба", async () => {
    const editing = await openParticlesProject({ hasFile: true });

    act(() => editing.current.renameParticleKind("дым", "туман"));
    await settle(editing);

    const written = writtenTexts();
    expect(written["particles.json"]).toBe(PARTICLES_TEXT.replace('"дым"', '"туман"'));
    expect((JSON.parse(written["scene.json"] as string).objects as Record<string, unknown>[]).map((object) => object.particles)).toEqual(["туман", "искры"]);
    expect(editing.current.selectedParticleName).toBe("туман");

    vi.mocked(writeProjectFile).mockClear();
    act(() => editing.current.undo());
    await settle(editing);

    expect(writtenTexts()).toEqual({ "particles.json": PARTICLES_TEXT, "scene.json": PARTICLE_SCENE_TEXT });
  });

  it("внешняя правка particles.json перечитывается как правка сцены", async () => {
    const editing = await openParticlesProject({ hasFile: true });
    const externalText = PARTICLES_TEXT.replace('"rate": 9', '"rate": 11');

    await act(async () => {
      projectEngineMock.onFullReload(
        { ...OK_RESULT, gameJsonText: GAME_JSON_WITH_PARTICLES, sceneText: PARTICLE_SCENE_TEXT } as unknown as ProjectLoadResult,
        (relativePath) => (relativePath === "particles.json" ? externalText : relativePath === "properties.json" ? "{}" : undefined),
        0,
      );
      await editing.current.whenIdle();
    });

    expect(editing.current.particlesText).toBe(externalText);
    expect(writeProjectFile).not.toHaveBeenCalled();
  });
});
