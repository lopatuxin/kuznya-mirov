import { mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { buildGamesFileResponse, isPathWithinDirectory, resolveGameFilePath } from "./vite.config";

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
