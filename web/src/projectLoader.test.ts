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
        stamps: [],
        code: "code.lua",
      },
      warnings: [{ file: "a", path: "", message: "предупреждение входа", line: null, column: null }] as EngineError[],
    }));
    const readTexts = vi.fn(() => ({
      fonts: [{ name: "Rubik", path: "fonts/Rubik.ttf" }],
      sounds: [{ index: 5, name: "beep", path: "sounds/beep.wav" }],
      music: [{ index: 7, name: "theme", path: "music/theme.mp3" }],
      images: [{ index: 9, name: "head", path: "images/head.png" }],
      materials: [],
      masks: [],
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
      undefined,
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
      [],
      [],
      [],
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
          stamps: [],
        },
        warnings: NO_WARNINGS,
      })),
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] })),
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
      [],
      [],
      [],
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
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [], terrain: terrainPath },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] })),
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

      expect(load.mock.calls[0]).toHaveLength(15);
      expect(load.mock.calls[0]?.[10]).toBeUndefined();
      expect(readText).not.toHaveBeenCalledWith("terrain.json");
    });

    it("файл назван, но не читается — в load идёт null: движок назовёт ошибку", async () => {
      const { engine, load } = createEngine("terrain.json");

      await loadProject(engine, createReader({}), "{}", stubAudioContext);

      expect(load.mock.calls[0]?.[10]).toBeNull();
    });
  });

  describe("файл видов частиц files.particles", () => {
    const files: Record<string, string> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "particles.json": '{"дым":{"image":"puff","rate":6,"lifetime":2,"size":1}}',
    };

    function createEngine(particlesPath: string | undefined): { engine: ProjectLoadEngine; load: ReturnType<typeof vi.fn> } {
      const load = vi.fn(() => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [], particles: particlesPath },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] })),
        load,
      };
      return { engine, load };
    }

    it("читается вторым заходом тем же читателем, что остальные файлы, и уходит в load последним аргументом", async () => {
      const { engine, load } = createEngine("particles.json");
      const recording = createRecordingProjectFileReader(createReader(files));

      await loadProject(engine, recording.reader, "{}", stubAudioContext);

      // Путь прошёл через читателя — значит, его же опрашивает редактор, и внешняя правка particles.json перечитывается.
      expect(recording.getReadPaths()).toContain("particles.json");
      expect(load.mock.calls[0]?.[14]).toBe(files["particles.json"]);
    });

    it("без files.particles в load идёт undefined, и файл не читается", async () => {
      const { engine, load } = createEngine(undefined);
      const readText = vi.fn(async () => null);

      await loadProject(engine, { readText, readBinary: async () => null }, "{}", stubAudioContext);

      expect(load.mock.calls[0]?.[14]).toBeUndefined();
      expect(readText).not.toHaveBeenCalledWith("particles.json");
    });

    it("файл назван, но не читается — в load идёт null: движок назовёт ошибку", async () => {
      const { engine, load } = createEngine("particles.json");

      await loadProject(engine, createReader({}), "{}", stubAudioContext);

      expect(load.mock.calls[0]?.[14]).toBeNull();
    });
  });

  describe("карты материалов и маски покрытий", () => {
    const files: Record<string, string | Uint8Array> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "terrain.json": '{"covers":[{"material":"grass"},{"material":"earth","mask":"terrain/earth.png"}],"heights":[[0]]}',
      "materials/grass/color.jpg": new Uint8Array([1]),
      "terrain/earth.png": new Uint8Array([2]),
    };

    function stubBrowserDecoder(): void {
      const bitmap = { width: 1, height: 1, close: vi.fn() };
      vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
      vi.stubGlobal("document", {
        createElement: () => ({
          width: 0,
          height: 0,
          getContext: () => ({ drawImage: vi.fn(), getImageData: () => ({ data: new Uint8ClampedArray([9, 8, 7, 255]) }) }),
        }),
      } as unknown as Document);
    }

    it("текст рельефа идёт в read_texts шестым аргументом, карты и маски читаются, разжимаются и уходят в load по номерам; ненайденный файл — missing", async () => {
      stubBrowserDecoder();
      const readTexts = vi.fn((..._args: unknown[]) => ({
        fonts: [],
        sounds: [],
        music: [],
        images: [],
        // Номера из read_texts — дело движка, нарочно не по возрастанию и не с нуля.
        materials: [
          { index: 4, path: "materials/grass/color.jpg" },
          { index: 2, path: "materials/grass/normal.jpg" },
        ],
        masks: [{ index: 7, path: "terrain/earth.png" }],
      }));
      const load = vi.fn((..._args: unknown[]) => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [], terrain: "terrain.json" },
          warnings: NO_WARNINGS,
        })),
        read_texts: readTexts,
        load,
      };

      await loadProject(engine, createReader(files), "{}", stubAudioContext);

      expect(readTexts.mock.calls[0]?.[5]).toBe(files["terrain.json"]);
      const decoded = { verdict: "ok", width: 1, height: 1, pixels: new Uint8Array([9, 8, 7, 255]) };
      expect(load.mock.calls[0]?.[11]).toEqual([
        { index: 4, ...decoded },
        { index: 2, verdict: "missing" },
      ]);
      expect(load.mock.calls[0]?.[12]).toEqual([{ index: 7, ...decoded }]);
    });

    it("в итоге загрузки маски покрытий — путь и красный канал точек; не найденная маска не входит («Покраска»)", async () => {
      stubBrowserDecoder();
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [], terrain: "terrain.json" },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({
          fonts: [],
          sounds: [],
          music: [],
          images: [],
          materials: [],
          masks: [
            { index: 0, path: "terrain/earth.png" },
            { index: 1, path: "terrain/absent.png" },
          ],
        })),
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      const result = await loadProject(engine, createReader(files), "{}", stubAudioContext);

      expect(result.status === "ok" && result.coverMasks).toEqual([{ path: "terrain/earth.png", width: 1, height: 1, pixels: new Uint8Array([9]) }]);
    });

    it("в итоге загрузки картинки files.images — имя, размер и точки; не найденная и не разжатая не входят («Редактор», вкладка «Картинки»)", async () => {
      stubBrowserDecoder();
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({
          fonts: [],
          sounds: [],
          music: [],
          images: [
            { index: 3, name: "izba", path: "images/izba.png" },
            { index: 4, name: "gone", path: "images/gone.png" },
          ],
          materials: [],
          masks: [],
        })),
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      const result = await loadProject(engine, createReader({ ...files, "images/izba.png": new Uint8Array([5]) }), "{}", stubAudioContext);

      expect(result.status === "ok" && result.images).toEqual([{ name: "izba", width: 1, height: 1, pixels: new Uint8Array([9, 8, 7, 255]) }]);
    });

    it("без файла рельефа в read_texts идёт undefined", async () => {
      const readTexts = vi.fn((..._args: unknown[]) => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
          warnings: NO_WARNINGS,
        })),
        read_texts: readTexts,
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      await loadProject(engine, createReader(files), "{}", stubAudioContext);

      expect(readTexts.mock.calls[0]).toHaveLength(6);
      expect(readTexts.mock.calls[0]?.[5]).toBeUndefined();
    });
  });

  describe("штампы отпечатков files.stamps", () => {
    const files: Record<string, string> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "stamps/beluha.json": '{"heights":[[0,1],[1,0]]}',
    };

    function createEngine(): { engine: ProjectLoadEngine; load: ReturnType<typeof vi.fn> } {
      const load = vi.fn((..._args: unknown[]) => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: {
            properties: "properties.json",
            scene: "scene.json",
            rules: "rules.json",
            screens: "screens.json",
            fonts: [],
            tables: [],
            stamps: [
              { name: "beluha", path: "stamps/beluha.json" },
              { name: "chuya", path: "stamps/chuya.json" },
            ],
          },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] })),
        load,
      };
      return { engine, load };
    }

    it("тексты читаются вторым проходом тем же читателем и уходят в load последним аргументом по порядку объявления; ненайденный файл — null", async () => {
      const { engine, load } = createEngine();
      const recording = createRecordingProjectFileReader(createReader(files));

      const result = await loadProject(engine, recording.reader, "{}", stubAudioContext);

      // Пути прошли через читателя — значит, их же опрашивает редактор, и правка штампа перезагружает проект.
      expect(recording.getReadPaths()).toEqual(expect.arrayContaining(["stamps/beluha.json", "stamps/chuya.json"]));
      const expected = [
        { name: "beluha", text: files["stamps/beluha.json"] },
        { name: "chuya", text: null },
      ];
      expect(load.mock.calls[0]?.[13]).toEqual(expected);
      expect(result.status === "ok" && result.stamps).toEqual(expected);
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
        files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
        warnings: NO_WARNINGS,
      })),
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [] })),
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
