import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { isPathWithinDirectory, resolveGameFilePath } from "./vite.config";

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
