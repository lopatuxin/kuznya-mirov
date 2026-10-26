import { afterEach, describe, expect, it, vi } from "vitest";
import type { EngineError } from "./engineErrors";
import { createRecordingProjectFileReader } from "./editor/recordingProjectFileReader";
import { FakeVideo } from "./images/fakeVideo";
import { createVideoPlayerKeeper } from "./images/videoLoader";
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

    const result = await loadProject(engine, reader, "{}", stubAudioContext, createVideoPlayerKeeper());

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
      models: [], model_textures: [],
    }));
    const load = vi.fn(() => ({
      ok: true,
      warnings: [{ file: "b", path: "", message: "предупреждение загрузки", line: null, column: null }] as EngineError[],
    }));
    const engine: ProjectLoadEngine = { read_entry: readEntry, read_texts: readTexts, load };
    const reader = createReader(files);

    const result = await loadProject(engine, reader, files["game.json"] as string, stubAudioContext, createVideoPlayerKeeper());

    expect(readEntry).toHaveBeenCalledWith(files["game.json"]);
    expect(readTexts).toHaveBeenCalledWith(
      files["properties.json"],
      files["scene.json"],
      files["rules.json"],
      files["screens.json"],
      files["code.lua"],
      undefined,
      null,
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
      [],
      [],
      [],
      null,
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
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] })),
      load,
    };
    const reader = createReader(files);

    await loadProject(engine, reader, files["game.json"] as string, stubAudioContext, createVideoPlayerKeeper());

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
      [],
      [],
      [],
      null,
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
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] })),
        load,
      };
      return { engine, load };
    }

    it("читается вторым проходом тем же читателем, что остальные файлы, и уходит в load последним аргументом", async () => {
      const { engine, load } = createEngine("terrain.json");
      const recording = createRecordingProjectFileReader(createReader(files));

      await loadProject(engine, recording.reader, "{}", stubAudioContext, createVideoPlayerKeeper());

      // Путь прошёл через читателя — значит, его же опрашивает редактор, и правка terrain.json перезагружает проект.
      expect(recording.getReadPaths()).toContain("terrain.json");
      expect(load.mock.calls[0]?.[10]).toBe(files["terrain.json"]);
    });

    it("без files.terrain в load идёт undefined, и файл не читается", async () => {
      const { engine, load } = createEngine(undefined);
      const readText = vi.fn(async () => null);

      await loadProject(engine, { readText, readBinary: async () => null }, "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(load.mock.calls[0]).toHaveLength(18);
      expect(load.mock.calls[0]?.[10]).toBeUndefined();
      expect(readText).not.toHaveBeenCalledWith("terrain.json");
    });

    it("файл назван, но не читается — в load идёт null: движок назовёт ошибку", async () => {
      const { engine, load } = createEngine("terrain.json");

      await loadProject(engine, createReader({}), "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(load.mock.calls[0]?.[10]).toBeNull();
    });
  });

  describe("частицы без файла видов", () => {
    it("particles.json не читается, а load получает ровно восемнадцать аргументов", async () => {
      const load = vi.fn(() => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
          warnings: NO_WARNINGS,
        })),
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] })),
        load,
      };
      const readText = vi.fn(async () => null);

      await loadProject(engine, { readText, readBinary: async () => null }, "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(readText).not.toHaveBeenCalledWith("particles.json");
      expect(load.mock.calls[0]).toHaveLength(18);
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
        models: [], model_textures: [],
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

      await loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper());

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
          models: [], model_textures: [],
        })),
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      const result = await loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper());

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
          models: [], model_textures: [],
        })),
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      const result = await loadProject(engine, createReader({ ...files, "images/izba.png": new Uint8Array([5]) }), "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(result.status === "ok" && result.images).toEqual([{ name: "izba", width: 1, height: 1, pixels: new Uint8Array([9, 8, 7, 255]) }]);
    });

    it("без файла рельефа в read_texts идёт undefined", async () => {
      const readTexts = vi.fn((..._args: unknown[]) => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
          warnings: NO_WARNINGS,
        })),
        read_texts: readTexts,
        load: vi.fn(() => ({ ok: true, warnings: NO_WARNINGS })),
      };

      await loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(readTexts.mock.calls[0]).toHaveLength(7);
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
        read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] })),
        load,
      };
      return { engine, load };
    }

    it("тексты читаются вторым проходом тем же читателем и уходят в load последним аргументом по порядку объявления; ненайденный файл — null", async () => {
      const { engine, load } = createEngine();
      const recording = createRecordingProjectFileReader(createReader(files));

      const result = await loadProject(engine, recording.reader, "{}", stubAudioContext, createVideoPlayerKeeper());

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

  describe("видео files.images", () => {
    const files: Record<string, string | Uint8Array> = {
      "game.json": "{}",
      "properties.json": "{}",
      "scene.json": '{"objects":[]}',
      "rules.json": "{}",
      "screens.json": "{}",
      "images/head.png": new Uint8Array([1]),
      "images/grass.mp4": new Uint8Array([2]),
    };
    let players: FakeVideo[] = [];

    /** `brokenImage` — разбор PNG 1 × 1 падает исключением на чтении точек, кадр видео 4 × 4 читается. */
    function stubBrowser({ brokenImage = false } = {}): void {
      players = [];
      const bitmap = { width: 1, height: 1, close: vi.fn() };
      vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
      const getImageData = (_x: number, _y: number, width: number, height: number) => {
        if (brokenImage && width === 1) throw new Error("не хватило памяти");
        return { data: new Uint8ClampedArray(width * height * 4) };
      };
      vi.stubGlobal("document", {
        createElement: (tag: string) => {
          if (tag === "video") {
            const player = new FakeVideo("loadeddata", 4, 4);
            players.push(player);
            return player;
          }
          return { width: 0, height: 0, getContext: () => ({ drawImage: vi.fn(), getImageData }) };
        },
      } as unknown as Document);
      vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:video");
      vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    }

    function createEngine(loadResult: { ok: true; warnings: EngineError[] } | { ok: false; errors: EngineError[]; warnings: EngineError[] }) {
      const load = vi.fn((..._args: unknown[]) => loadResult);
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({
          ok: true,
          files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
          warnings: NO_WARNINGS,
        })),
        // Номера из read_texts — дело движка, нарочно не по порядку.
        read_texts: vi.fn(() => ({
          fonts: [],
          sounds: [],
          music: [],
          images: [
            { index: 3, name: "grass", path: "images/grass.mp4" },
            { index: 1, name: "head", path: "images/head.png" },
            { index: 6, name: "gone", path: "images/gone.MP4" },
          ],
          materials: [],
          masks: [],
          models: [], model_textures: [],
        })),
        load,
      };
      return { engine, load };
    }

    it(".mp4 уходит в load только пятнадцатым аргументом: проигрыватель, размер файла и точки первого кадра при ok, missing без файла; PNG — только в images", async () => {
      stubBrowser();
      const { engine, load } = createEngine({ ok: true, warnings: NO_WARNINGS });

      await loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper());

      const args = load.mock.calls[0] ?? [];
      expect(args[7]).toEqual([expect.objectContaining({ index: 1, verdict: "ok" })]);
      // Кадр файла 4 × 4 — 4 × 2 точки по четыре байта: по ним движок решает, откуда срываются листья.
      expect((args[14] as { pixels: Uint8Array }[])[0]?.pixels).toHaveLength(4 * 2 * 4);
      expect(args[14]).toEqual([
        { index: 3, verdict: "ok", width: 4, height: 4, player: players[0], pixels: expect.any(Uint8Array) },
        { index: 6, verdict: "missing" },
      ]);
    });

    describe("видео панели с reverse", () => {
      const reverseFiles: Record<string, string | Uint8Array> = {
        ...files,
        "images/hero.mp4": new Uint8Array([3]),
        "images/hero-reverse.mp4": new Uint8Array([4]),
      };

      function createReverseEngine() {
        const { engine, load } = createEngine({ ok: true, warnings: NO_WARNINGS });
        engine.read_texts = vi.fn(() => ({
          fonts: [],
          sounds: [],
          music: [],
          images: [
            { index: 2, name: "hero", path: "images/hero.mp4", reverse: "images/hero-reverse.mp4" },
            { index: 5, name: "lost", path: "images/lost.mp4", reverse: "images/lost-reverse.mp4" },
          ],
          materials: [],
          masks: [],
          models: [], model_textures: [],
        }));
        return { engine, load };
      }

      it("обратный файл читается и уходит в load в поле reverse ответа по видео; нет файла — reverse не создаётся", async () => {
        stubBrowser();
        const { engine, load } = createReverseEngine();

        await loadProject(engine, createReader(reverseFiles), "{}", stubAudioContext, createVideoPlayerKeeper());

        expect(players).toHaveLength(2);
        expect((load.mock.calls[0] ?? [])[14]).toEqual([
          { index: 2, verdict: "ok", width: 4, height: 4, player: players[0], pixels: expect.any(Uint8Array), reverse: { verdict: "ok", player: players[1] } },
          { index: 5, verdict: "missing" },
        ]);
      });

      it("новая загрузка отпускает и обычный, и обратный проигрыватель прежней", async () => {
        stubBrowser();
        const { engine } = createReverseEngine();
        const keeper = createVideoPlayerKeeper();

        await loadProject(engine, createReader(reverseFiles), "{}", stubAudioContext, keeper);
        await loadProject(engine, createReader(reverseFiles), "{}", stubAudioContext, keeper);

        expect(players).toHaveLength(4);
        expect(players[0]?.pause).toHaveBeenCalled();
        expect(players[1]?.pause).toHaveBeenCalled();
        expect(players[2]?.pause).not.toHaveBeenCalled();
        expect(players[3]?.pause).not.toHaveBeenCalled();
      });

      it("отказ load освобождает оба новых проигрывателя", async () => {
        stubBrowser();
        const { engine } = createReverseEngine();
        engine.load = vi.fn(() => ({ ok: false as const, errors: [], warnings: NO_WARNINGS }));

        await loadProject(engine, createReader(reverseFiles), "{}", stubAudioContext, createVideoPlayerKeeper());

        expect(players).toHaveLength(2);
        expect(players[0]?.pause).toHaveBeenCalled();
        expect(players[1]?.pause).toHaveBeenCalled();
      });
    });

    it("редактору видео достаётся кадром: ширина файла, половина высоты и своё имя; картинка — как раньше", async () => {
      stubBrowser();
      const { engine } = createEngine({ ok: true, warnings: NO_WARNINGS });

      const result = await loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper());

      if (result.status !== "ok") throw new Error("unreachable");
      expect(result.images.map(({ name, width, height }) => ({ name, width, height }))).toEqual([
        { name: "grass", width: 4, height: 2 },
        { name: "head", width: 1, height: 1 },
      ]);
    });

    it("новая загрузка с тем же хранителем отпускает прежние проигрыватели, но не свои", async () => {
      stubBrowser();
      const { engine } = createEngine({ ok: true, warnings: NO_WARNINGS });
      const keeper = createVideoPlayerKeeper();

      await loadProject(engine, createReader(files), "{}", stubAudioContext, keeper);
      await loadProject(engine, createReader(files), "{}", stubAudioContext, keeper);

      expect(players).toHaveLength(2);
      expect(players[0]?.pause).toHaveBeenCalled();
      expect(players[0]?.removeAttribute).toHaveBeenCalledWith("src");
      expect(players[1]?.pause).not.toHaveBeenCalled();
    });

    it("отказ load освобождает и новые проигрыватели, и прежние", async () => {
      stubBrowser();
      const keeper = createVideoPlayerKeeper();
      await loadProject(createEngine({ ok: true, warnings: NO_WARNINGS }).engine, createReader(files), "{}", stubAudioContext, keeper);
      const errors: EngineError[] = [{ file: "game.json", path: "files → images → grass", message: "плохо", line: null, column: null }];

      const result = await loadProject(createEngine({ ok: false, errors, warnings: NO_WARNINGS }).engine, createReader(files), "{}", stubAudioContext, keeper);

      expect(result.status).toBe("rejected");
      expect(players).toHaveLength(2);
      expect(players[0]?.pause).toHaveBeenCalled();
      expect(players[1]?.pause).toHaveBeenCalled();
    });

    it("исключение в load освобождает новые проигрыватели и уходит дальше", async () => {
      stubBrowser();
      const { engine } = createEngine({ ok: true, warnings: NO_WARNINGS });
      engine.load = vi.fn(() => {
        throw new Error("движок упал");
      });

      await expect(loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper())).rejects.toThrow("движок упал");

      expect(players).toHaveLength(1);
      expect(players[0]?.pause).toHaveBeenCalled();
      expect(players[0]?.removeAttribute).toHaveBeenCalledWith("src");
    });

    it("сбой разбора картинки рядом с видео освобождает проигрыватель видео", async () => {
      stubBrowser({ brokenImage: true });
      const { engine, load } = createEngine({ ok: true, warnings: NO_WARNINGS });

      await expect(loadProject(engine, createReader(files), "{}", stubAudioContext, createVideoPlayerKeeper())).rejects.toThrow("не хватило памяти");

      expect(load).not.toHaveBeenCalled();
      await vi.waitFor(() => expect(players[0]?.pause).toHaveBeenCalled());
      expect(players[0]?.removeAttribute).toHaveBeenCalledWith("src");
    });
  });

  it("модели читаются байтами по путям из read_texts и уходят в load шестнадцатым аргументом по их номерам; ненайденный файл — bytes null", async () => {
    const houseBytes = new Uint8Array([1, 2, 3]);
    const load = vi.fn((..._args: unknown[]) => ({ ok: true, warnings: NO_WARNINGS }));
    const engine: ProjectLoadEngine = {
      read_entry: vi.fn(() => ({
        ok: true,
        files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] },
        warnings: NO_WARNINGS,
      })),
      // Номера из read_texts — дело движка, нарочно не по порядку.
      read_texts: vi.fn(() => ({
        fonts: [],
        sounds: [],
        music: [],
        images: [],
        materials: [],
        masks: [],
        models: [
          { index: 3, name: "house", path: "models/house.glb" },
          { index: 1, name: "birch", path: "models/birch.glb" },
        ],
        model_textures: [],
      })),
      load,
    };

    await loadProject(engine, createReader({ "models/house.glb": houseBytes }), "{}", stubAudioContext, createVideoPlayerKeeper());

    expect(load.mock.calls[0]?.[15]).toEqual([
      { index: 3, bytes: houseBytes, images: [] },
      { index: 1, bytes: null, images: [] },
    ]);
  });

  describe("сжатые модели — imported/", () => {
    const INDEX_TEXT = JSON.stringify({ version: 1, models: { "models/house.glb": { model: "imported/models/house.glb", images: ["imported/textures/a.ktx2", null] } } });
    const ENTRY_FILES = { properties: "properties.json", scene: "scene.json", rules: "rules.json", screens: "screens.json", fonts: [], tables: [], stamps: [] };

    function createEngine(models: unknown[], modelTextures: unknown[]): { engine: ProjectLoadEngine; readTexts: ReturnType<typeof vi.fn>; load: ReturnType<typeof vi.fn> } {
      const readTexts = vi.fn((..._args: unknown[]) => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models, model_textures: modelTextures }));
      const load = vi.fn((..._args: unknown[]) => ({ ok: true, warnings: NO_WARNINGS }));
      const engine: ProjectLoadEngine = {
        read_entry: vi.fn(() => ({ ok: true, files: ENTRY_FILES, warnings: NO_WARNINGS })),
        read_texts: readTexts,
        load,
      };
      return { engine, readTexts, load };
    }

    it("текст imported/index.json идёт в read_texts седьмым и в load последним; сжатые файлы читаются байтами по номерам из read_texts и уходят как есть, нет файла или он пуст — bytes null", async () => {
      const lightBytes = new Uint8Array([1, 2, 3]);
      const textureA = new Uint8Array([10, 11]);
      const textureB = new Uint8Array([12]);
      const { engine, readTexts, load } = createEngine(
        [{ index: 4, name: "house", path: "imported/models/house.glb" }],
        [
          { index: 7, path: "imported/textures/a.ktx2" },
          { index: 2, path: "imported/textures/b.ktx2" },
          { index: 5, path: "imported/textures/missing.ktx2" },
          { index: 6, path: "imported/textures/empty.ktx2" },
        ],
      );
      const reader = createReader({
        "imported/index.json": INDEX_TEXT,
        "imported/models/house.glb": lightBytes,
        "imported/textures/a.ktx2": textureA,
        "imported/textures/b.ktx2": textureB,
        "imported/textures/empty.ktx2": new Uint8Array(),
      });

      await loadProject(engine, reader, "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(readTexts.mock.calls[0]?.[6]).toBe(INDEX_TEXT);
      expect(load.mock.calls[0]?.[15]).toEqual([{ index: 4, bytes: lightBytes, images: [] }]);
      const modelTextures = load.mock.calls[0]?.[16] as { index: number; bytes: Uint8Array | null }[];
      expect(modelTextures).toEqual([
        { index: 7, bytes: textureA },
        { index: 2, bytes: textureB },
        { index: 5, bytes: null },
        { index: 6, bytes: null },
      ]);
      expect(modelTextures[0]?.bytes).toBe(textureA);
      expect(load.mock.calls[0]?.[17]).toBe(INDEX_TEXT);
    });

    it("без imported/index.json в read_texts и load уходит null, моделей-текстур нет, исходная модель читается как прежде", async () => {
      const houseBytes = new Uint8Array([1, 2, 3]);
      const { engine, readTexts, load } = createEngine([{ index: 0, name: "house", path: "models/house.glb" }], []);

      await loadProject(engine, createReader({ "models/house.glb": houseBytes }), "{}", stubAudioContext, createVideoPlayerKeeper());

      expect(readTexts.mock.calls[0]?.[6]).toBeNull();
      expect(load.mock.calls[0]?.[15]).toEqual([{ index: 0, bytes: houseBytes, images: [] }]);
      expect(load.mock.calls[0]?.[16]).toEqual([]);
      expect(load.mock.calls[0]?.[17]).toBeNull();
    });

    it("опрос файлов редактора видит imported/index.json даже без файла, а с ним — облегчённую модель и сжатые файлы, но не исходную модель", async () => {
      const withoutIndex = createEngine([{ index: 0, name: "house", path: "models/house.glb" }], []);
      const missingRecording = createRecordingProjectFileReader(createReader({ "models/house.glb": new Uint8Array([1]) }));
      await loadProject(withoutIndex.engine, missingRecording.reader, "{}", stubAudioContext, createVideoPlayerKeeper());
      expect(missingRecording.getReadPaths()).toContain("imported/index.json");

      const withIndex = createEngine([{ index: 0, name: "house", path: "imported/models/house.glb" }], [{ index: 0, path: "imported/textures/a.ktx2" }]);
      const recording = createRecordingProjectFileReader(createReader({ "imported/index.json": INDEX_TEXT }));
      await loadProject(withIndex.engine, recording.reader, "{}", stubAudioContext, createVideoPlayerKeeper());
      expect(recording.getReadPaths()).toEqual(expect.arrayContaining(["imported/index.json", "imported/models/house.glb", "imported/textures/a.ktx2"]));
      expect(recording.getReadPaths()).not.toContain("models/house.glb");
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
      read_texts: vi.fn(() => ({ fonts: [], sounds: [], music: [], images: [], materials: [], masks: [], models: [], model_textures: [] })),
      load: vi.fn(() => ({ ok: false, errors, warnings: NO_WARNINGS })),
    };
    const reader = createReader(files);

    const result = await loadProject(engine, reader, files["game.json"], stubAudioContext, createVideoPlayerKeeper());

    expect(result).toEqual({
      status: "rejected",
      errors,
      warnings: NO_WARNINGS,
      gameJsonText: files["game.json"],
      sceneText: files["scene.json"],
    });
  });
});
