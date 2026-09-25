import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { buildGamesFileResponse, isPathWithinDirectory, resolveGameFilePath, resolvePutTarget, writeGamesFileAtomically } from "./vite.config";

// Файл называется не vite.config.*, потому что vitest по умолчанию исключает из тестов любой
// файл вида vite.config.* (чтобы не принять сам конфиг за тест) — под этот шаблон попал бы и
// vite.config.test.ts.
const gamesDir = resolve("/project/games");

describe("isPathWithinDirectory", () => {
  it("пропускает путь внутри каталога", () => {
    expect(isPathWithinDirectory(resolve(gamesDir, "snake/game.json"), gamesDir)).toBe(true);
  });

  it("пропускает сам каталог", () => {
    expect(isPathWithinDirectory(gamesDir, gamesDir)).toBe(true);
  });

  it("не пропускает путь, вышедший через .. за пределы каталога", () => {
    expect(isPathWithinDirectory(resolve(gamesDir, "../secret"), gamesDir)).toBe(false);
  });

  it("не пропускает соседний каталог с похожим именем", () => {
    expect(isPathWithinDirectory(resolve(gamesDir, "../gamesX/file.json"), gamesDir)).toBe(false);
  });
});

describe("resolveGameFilePath", () => {
  it("даёт null на битой процентной кодировке вместо падения", () => {
    expect(resolveGameFilePath("/%ZZ", gamesDir)).toBe(null);
  });

  it("даёт null на пути с нулевым байтом вместо падения", () => {
    expect(resolveGameFilePath("/snake/game.json%00.txt", gamesDir)).toBe(null);
  });
});

describe("buildGamesFileResponse", () => {
  let dir: string | null = null;

  afterEach(() => {
    if (dir !== null) rmSync(dir, { recursive: true, force: true });
    dir = null;
  });

  it("отдаёт ETag и Last-Modified по размеру и времени изменения файла", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-"));
    const filePath = join(dir, "scene.json");
    writeFileSync(filePath, "{}");

    const response = buildGamesFileResponse(filePath, "GET");

    expect(response.headers["Content-Length"]).toBe("2");
    expect(response.headers.ETag).toBeTruthy();
    expect(response.headers["Last-Modified"]).toBeTruthy();
    expect(response.body?.toString()).toBe("{}");
  });

  it("отвечает на HEAD без тела, но с теми же заголовками", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-"));
    const filePath = join(dir, "scene.json");
    writeFileSync(filePath, "{}");

    const response = buildGamesFileResponse(filePath, "HEAD");

    expect(response.body).toBe(null);
    expect(response.headers.ETag).toBeTruthy();
    expect(response.headers["Last-Modified"]).toBeTruthy();
  });

  it("меняет ETag и Last-Modified после правки файла той же длины", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-"));
    const filePath = join(dir, "scene.json");
    writeFileSync(filePath, "{\"a\":1}");
    const before = buildGamesFileResponse(filePath, "HEAD");

    // Содержимое той же длины, чтобы ETag не мог измениться просто из-за Content-Length, и
    // явное время изменения через utimesSync, чтобы не зависеть от разрешения часов файловой
    // системы между двумя writeFileSync подряд.
    writeFileSync(filePath, "{\"a\":2}");
    utimesSync(filePath, new Date(Date.now() + 60_000), new Date(Date.now() + 60_000));
    const after = buildGamesFileResponse(filePath, "HEAD");

    expect(after.headers["Content-Length"]).toBe(before.headers["Content-Length"]);
    expect(after.headers.ETag).not.toBe(before.headers.ETag);
    expect(after.headers["Last-Modified"]).not.toBe(before.headers["Last-Modified"]);
  });
});

describe("resolvePutTarget", () => {
  it("путь внутри games/<имя>/ на .json — ok", () => {
    expect(resolvePutTarget("/tetris/scene.json", gamesDir)).toEqual({
      status: "ok",
      filePath: resolve(gamesDir, "tetris/scene.json"),
      createDirectory: false,
    });
  });

  it("файл в подпапке проекта на любой глубине — ok (фаза 09: files.scene/properties могут лежать не прямо в папке)", () => {
    expect(resolvePutTarget("/tetris/assets/wall.json", gamesDir)).toEqual({
      status: "ok",
      filePath: resolve(gamesDir, "tetris/assets/wall.json"),
      createDirectory: false,
    });
  });

  it("путь с .. — forbidden", () => {
    expect(resolvePutTarget("/tetris/../../secret.json", gamesDir)).toEqual({ status: "forbidden" });
  });

  it("битая процентная кодировка — forbidden", () => {
    expect(resolvePutTarget("/tetris/%ZZ.json", gamesDir)).toEqual({ status: "forbidden" });
  });

  it("нулевой байт в пути — forbidden", () => {
    expect(resolvePutTarget("/tetris/scene.json%00.txt", gamesDir)).toEqual({ status: "forbidden" });
  });

  it("сам каталог games — forbidden", () => {
    expect(resolvePutTarget("/", gamesDir)).toEqual({ status: "forbidden" });
  });

  it("файл прямо в games/, без папки проекта — not-allowed", () => {
    expect(resolvePutTarget("/index.json", gamesDir)).toEqual({ status: "not-allowed" });
  });

  it("не .json — not-allowed", () => {
    expect(resolvePutTarget("/tetris/wall.png", gamesDir)).toEqual({ status: "not-allowed" });
  });

  describe("replays/", () => {
    let dir: string | null = null;

    afterEach(() => {
      if (dir !== null) rmSync(dir, { recursive: true, force: true });
      dir = null;
    });

    it("запись в replays/ существующего проекта — ok, папку можно создать (требование 34)", () => {
      dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-target-"));
      mkdirSync(join(dir, "tetris"));

      expect(resolvePutTarget("/tetris/replays/2026-03-04-09-05-07.json", dir)).toEqual({
        status: "ok",
        filePath: resolve(dir, "tetris/replays/2026-03-04-09-05-07.json"),
        createDirectory: true,
      });
    });

    it("запись в replays/ несуществующего проекта — forbidden", () => {
      dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-target-"));

      expect(resolvePutTarget("/tetris/replays/2026-03-04-09-05-07.json", dir)).toEqual({ status: "forbidden" });
    });

    it("вложенная папка внутри replays/ — ok формой, но без создания папки (как у любого пути без папки, фаза 09)", () => {
      dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-target-"));
      mkdirSync(join(dir, "tetris"));

      expect(resolvePutTarget("/tetris/replays/sub/2026-03-04-09-05-07.json", dir)).toEqual({
        status: "ok",
        filePath: resolve(dir, "tetris/replays/sub/2026-03-04-09-05-07.json"),
        createDirectory: false,
      });
    });
  });
});

describe("writeGamesFileAtomically", () => {
  let dir: string | null = null;

  afterEach(() => {
    if (dir !== null) rmSync(dir, { recursive: true, force: true });
    dir = null;
  });

  it("заменяет содержимое файла и не оставляет временного файла", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-"));
    const filePath = join(dir, "scene.json");
    writeFileSync(filePath, '{"objects":[]}');

    return writeGamesFileAtomically(filePath, Buffer.from('{"objects":[1]}'), false).then(() => {
      expect(readFileSync(filePath, "utf8")).toBe('{"objects":[1]}');
      const leftovers = readdirSync(dir as string).filter((name) => name.includes(".tmp-"));
      expect(leftovers).toEqual([]);
    });
  });

  it("пишет новый файл в существующей папке проекта", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-"));
    mkdirSync(join(dir, "tetris"));
    const filePath = join(dir, "tetris", "scene.json");

    return writeGamesFileAtomically(filePath, Buffer.from("{}"), false).then(() => {
      expect(existsSync(filePath)).toBe(true);
      expect(readFileSync(filePath, "utf8")).toBe("{}");
    });
  });

  it("без createDirectory не создаёт недостающую папку — отказ, как в фазе 09", async () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-"));
    const filePath = join(dir, "tetris", "scene.json");

    await expect(writeGamesFileAtomically(filePath, Buffer.from("{}"), false)).rejects.toThrow();
    expect(existsSync(filePath)).toBe(false);
  });

  it("создаёт replays/ на самой первой записи партии — требование 34", () => {
    dir = mkdtempSync(join(tmpdir(), "kuznya-games-put-"));
    mkdirSync(join(dir, "tetris"));
    const filePath = join(dir, "tetris", "replays", "2026-03-04-09-05-07.json");

    return writeGamesFileAtomically(filePath, Buffer.from('{"format":1,"steps":0,"events":[]}'), true).then(() => {
      expect(readFileSync(filePath, "utf8")).toBe('{"format":1,"steps":0,"events":[]}');
    });
  });
});
