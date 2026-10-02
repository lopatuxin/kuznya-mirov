import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { resolveCommandPath, writeStrokeFiles } from "./strokeFiles";

let workFolder: string | null = null;

afterEach(() => {
  if (workFolder !== null) rmSync(workFolder, { recursive: true, force: true });
  workFolder = null;
});

function makeWorkFolder(): string {
  workFolder = mkdtempSync(join(tmpdir(), "stroke-files-"));
  return workFolder;
}

describe("пути команды — от папки, где она запущена (требование 22)", () => {
  it("npm run отдаёт папку запуска в INIT_CWD — путь считается от неё, а не от web/", () => {
    const launch = resolve("launch-folder");

    expect(resolveCommandPath("games/rpg", { INIT_CWD: launch }, resolve("web"))).toBe(join(launch, "games", "rpg"));
  });

  it("без INIT_CWD — от рабочей папки процесса", () => {
    expect(resolveCommandPath("strokes.json", {}, resolve("here"))).toBe(join(resolve("here"), "strokes.json"));
  });

  it("абсолютный путь остаётся как есть", () => {
    const absolute = resolve("somewhere", "strokes.json");

    expect(resolveCommandPath(absolute, { INIT_CWD: resolve("launch-folder") }, resolve("web"))).toBe(absolute);
  });
});

describe("запись файлов команды", () => {
  it("пишет текст и байты, создаёт папку маски, заменяет прежний файл и не оставляет временных", () => {
    const folder = makeWorkFolder();
    writeFileSync(join(folder, "terrain.json"), "старый");

    writeStrokeFiles(folder, [
      { path: "terrain/rock.png", content: Uint8Array.of(137, 80, 78, 71) },
      { path: "terrain.json", content: "новый" },
    ]);

    expect(Array.from(readFileSync(join(folder, "terrain", "rock.png")))).toEqual([137, 80, 78, 71]);
    expect(readFileSync(join(folder, "terrain.json"), "utf8")).toBe("новый");
    expect(readdirSync(folder).sort()).toEqual(["terrain", "terrain.json"]);
    expect(readdirSync(join(folder, "terrain"))).toEqual(["rock.png"]);
  });
});
