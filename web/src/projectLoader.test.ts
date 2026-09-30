import { afterEach, describe, expect, it, vi } from "vitest";
import type { EngineError } from "./engineErrors";
import { createRecordingProjectFileReader } from "./editor/recordingProjectFileReader";
import { loadProject, type ProjectFileReader, type ProjectLoadEngine } from "./projectLoader";

const NO_WARNINGS: EngineError[] = [];

function stubAudioContext(): AudioContext {
  return {
    decodeAudioData: vi.fn(async () => {
      throw new Error("не годится");
    }),
  } as unknown as AudioContext;
}

function createReader(files: Record<string, string | Uint8Array>): ProjectFileReader {
  return {
    readText: async (relativePath) => {
      const entry = files[relativePath];
      return typeof entry === "string" ? entry : null;
    },
    readBinary: async (relativePath) => {
      const entry = files[relativePath];
      return entry instanceof Uint8Array ? entry : null;
    },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("loadProject", () => {
  it("отдаёт rejected с ошибками read_entry, не заходя на read_texts/load", async () => {
    const errors: EngineError[] = [{ file: "game.json", path: "", message: "плохо", line: null, column: null }];
    const readTexts = vi.fn();
    const load = vi.fn();
    const engine: ProjectLoadEngine = {
      read_entry: vi.fn(() => ({ ok: false, errors, warnings: NO_WARNINGS })),
      read_texts: readTexts,
      load,
    };
    const reader = createReader({});

    const result = await loadProject(engine, reader, "{}", stubAudioContext);

    expect(result).toEqual({ status: "rejected", errors, warnings: NO_WARNINGS, gameJsonText: "{}", sceneText: null });
    expect(readTexts).not.toHaveBeenCalled();
    expect(load).not.toHaveBeenCalled();
  });

  it("проходит все три захода в порядке read_entry → read_texts → load, отдаёт load те же файлы, что назвал read_texts, и читает их по путям из read_entry/read_texts", async () => {
    const fontBytes = new Uint8Array([1]);
    const soundBytes = new Uint8Array([2]);
    const musicBytes = new Uint8Array([3]);
    const imageBytes = new Uint8Array([4]);
    const files: Record<string, string | Uint8Array> = {
      "game.json": '{"name":"Т"}',
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "code.lua": "-- код",
      "fonts/Rubik.ttf": fontBytes,
      "sounds/beep.wav": soundBytes,
      "music/theme.mp3": musicBytes,
      "images/head.png": imageBytes,
      "tables/enemies.json": '{"goblin":{"health":30}}',
    };
    const readEntry = vi.fn(() => ({
      ok: true,
      files: {
        properties: "properties.json",
        scene: "scene.json",
        rules: "rules.json",
        screens: "screens.json",
        fonts: [{ name: "Rubik", path: "fonts/Rubik.ttf" }],
        tables: [{ name: "enemies", path: "tables/enemies.json" }],
        code: "code.lua",
      },
      warnings: [{ file: "a", path: "", message: "предупреждение входа", line: null, column: null }] as EngineError[],
    }));
    const readTexts = vi.fn(() => ({
      fonts: [{ name: "Rubik", path: "fonts/Rubik.ttf" }],
      sounds: [{ index: 5, name: "beep", path: "sounds/beep.wav" }],
      music: [{ index: 7, name: "theme", path: "music/theme.mp3" }],
      images: [{ index: 9, name: "head", path: "images/head.png" }],
    }));
    const load = vi.fn(() => ({
      ok: true,
      warnings: [{ file: "b", path: "", message: "предупреждение загрузки", line: null, column: null }] as EngineError[],
    }));
    const engine: ProjectLoadEngine = { read_entry: readEntry, read_texts: readTexts, load };
    const reader = createReader(files);

    const result = await loadProject(engine, reader, files["game.json"] as string, stubAudioContext);

    expect(readEntry).toHaveBeenCalledWith(files["game.json"]);
    expect(readTexts).toHaveBeenCalledWith(
      files["properties.json"],
      files["scene.json"],
      files["rules.json"],
      files["screens.json"],
      files["code.lua"],
    );
    // load получает шрифты, звуки, приговоры музыке, картинки и код — «Редактор», требование 11.
    expect(load).toHaveBeenCalledWith(
      files["properties.json"],
      files["scene.json"],
      files["rules.json"],
      files["screens.json"],
      [{ name: "Rubik", bytes: fontBytes }],
      [{ index: 5, path: "sounds/beep.wav", bytes: soundBytes }],
      [{ index: 7, verdict: "rejected" }],
      [expect.objectContaining({ index: 9 })],
      files["code.lua"],
      [{ name: "enemies", text: files["tables/enemies.json"] }],
      undefined,
    );
    expect(result.status).toBe("ok");
    if (result.status !== "ok") throw new Error("unreachable");
    expect(result.sceneText).toBe(files["scene.json"]);
    // read_entry первым, load вторым — тот же порядок, что собирает сам движок.
    expect(result.warnings.map((warning) => warning.message)).toEqual(["предупреждение входа", "предупреждение загрузки"]);
  });

  it("таблица, файл которой не читается, идёт в load текстом null", async () => {
    const files: Record<string, string | Uint8Array> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
    };
    const load = vi.fn(() => ({ ok: true, warnings: NO_WARNINGS }));
    const engine: ProjectLoadEngine = {
      read_entry: vi.fn(() => ({
        ok: true,
        files: {
          properties: "properties.json",
          scene: "scene.json",
          rules: "rules.json",
          screens: "screens.json",
          fonts: [],
          tables: [{ name: "enemies", path: "tables/enemies.json" }],
        },
        warnings: NO_WARNINGS,
      })),
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [] })),
      load,
    };
    const reader = createReader(files);

    await loadProject(engine, reader, files["game.json"] as string, stubAudioContext);

    expect(load).toHaveBeenCalledWith(
      files["properties.json"],
      files["scene.json"],
      files["rules.json"],
      files["screens.json"],
      [],
      [],
      [],
      [],
      null,
      [{ name: "enemies", text: null }],
      undefined,
    );
  });

  describe("файл рельефа files.terrain", () => {
    const files: Record<string, string> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "terrain.json": '{"heights":[[0]]}',
    };

    function createEngine(terrainPath: string | undefined): { engine: ProjectLoadEngine; load: ReturnType<typeof vi.fn> } {
      const load = vi.fn(() => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], terrain: terrainPath },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [] })),
        load,
      };
      return { engine, load };
    }

    it("читается вторым проходом тем же читателем, что остальные файлы, и уходит в load последним аргументом", async () => {
      const { engine, load } = createEngine("terrain.json");
      const recording = createRecordingProjectFileReader(createReader(files));

      await loadProject(engine, recording.reader, "{}", stubAudioContext);

      // Путь прошёл через читателя — значит, его же опрашивает редактор, и правка terrain.json перезагружает проект.
      expect(recording.getReadPaths()).toContain("terrain.json");
      expect(load.mock.calls[0]?.[10]).toBe(files["terrain.json"]);
    });

    it("без files.terrain в load идёт undefined, и файл не читается", async () => {
      const { engine, load } = createEngine(undefined);
      const readText = vi.fn(async () => null);

      await loadProject(engine, { readText, readBinary: async () => null }, "{}", stubAudioContext);

      expect(load.mock.calls[0]).toHaveLength(11);
      expect(load.mock.calls[0]?.[10]).toBeUndefined();
      expect(readText).not.toHaveBeenCalledWith("terrain.json");
    });

    it("файл назван, но не читается — в load идёт null: движок назовёт ошибку", async () => {
      const { engine, load } = createEngine("terrain.json");

      await loadProject(engine, createReader({}), "{}", stubAudioContext);

      expect(load.mock.calls[0]?.[10]).toBeNull();
    });
  });

  it("отдаёт rejected с текстом сцены, когда read_entry прошёл, а load отказал", async () => {
    const files = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
    };
    const errors: EngineError[] = [{ file: "scene.json", path: "", message: "плохо", line: null, column: null }];
    const engine: ProjectLoadEngine = {
      read_entry: vi.fn(() => ({
        ok: true,
        files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [] },
        warnings: NO_WARNINGS,
      })),
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [] })),
      load: vi.fn(() => ({ ok: false, errors, warnings: NO_WARNINGS })),
    };
    const reader = createReader(files);

    const result = await loadProject(engine, reader, files["game.json"], stubAudioContext);

    expect(result).toEqual({
      status: "rejected",
      errors,
      warnings: NO_WARNINGS,
      gameJsonText: files["game.json"],
      sceneText: files["scene.json"],
    });
  });
});
