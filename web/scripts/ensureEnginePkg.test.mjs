import { mkdtempSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { isEnginePkgStale, latestMtimeMs } from "./ensureEnginePkg.mjs";

let workDir;

afterEach(() => {
  if (workDir) rmSync(workDir, { recursive: true, force: true });
});

function touch(path, secondsSinceEpoch) {
  writeFileSync(path, "");
  utimesSync(path, secondsSinceEpoch, secondsSinceEpoch);
}

describe("latestMtimeMs", () => {
  it("недостающий путь не даёт вклада в максимум — успех на пути, которого нет", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    expect(latestMtimeMs(join(workDir, "отсутствует"))).toBe(-Infinity);
  });

  it("для файла возвращает его собственное время правки", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const filePath = join(workDir, "Cargo.toml");
    touch(filePath, 1_000);
    expect(latestMtimeMs(filePath)).toBe(1_000_000);
  });

  it("для каталога обходит вложенные файлы рекурсивно и берёт самое позднее время", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const srcDir = join(workDir, "src");
    const nestedDir = join(srcDir, "stages");
    mkdirSync(nestedDir, { recursive: true });
    touch(join(srcDir, "lib.rs"), 1_000);
    touch(join(nestedDir, "physics.rs"), 2_000);
    // Создание файлов и вложенного каталога только что обновило mtime самих каталогов на
    // реальное «сейчас» — выставляем его пораньше, чтобы максимум определялся файлами внутри,
    // как в реальном дереве исходников, а не моментом подготовки теста.
    utimesSync(srcDir, 500, 500);
    utimesSync(nestedDir, 500, 500);
    expect(latestMtimeMs(srcDir)).toBe(2_000_000);
  });
});

describe("isEnginePkgStale", () => {
  it("отсутствующий pkg считается устаревшим — успех без собранного pkg", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const pkgManifest = join(workDir, "pkg", "package.json");
    expect(isEnginePkgStale(pkgManifest, [join(workDir, "src")])).toBe(true);
  });

  it("исходники новее собранного pkg — pkg устарел", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const srcPath = join(workDir, "src.rs");
    const pkgManifest = join(workDir, "pkg", "package.json");
    mkdirSync(join(workDir, "pkg"), { recursive: true });
    touch(pkgManifest, 1_000);
    touch(srcPath, 2_000);
    expect(isEnginePkgStale(pkgManifest, [srcPath])).toBe(true);
  });

  it("pkg собран после последней правки исходников — не устарел", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const srcPath = join(workDir, "src.rs");
    const pkgManifest = join(workDir, "pkg", "package.json");
    mkdirSync(join(workDir, "pkg"), { recursive: true });
    touch(srcPath, 1_000);
    touch(pkgManifest, 2_000);
    expect(isEnginePkgStale(pkgManifest, [srcPath])).toBe(false);
  });

  it("правка файла в luars/src (соседний с engine каталог) делает pkg устаревшим", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const engineDir = join(workDir, "engine");
    const luarsSrcDir = join(workDir, "luars", "src");
    const pkgManifest = join(engineDir, "pkg", "package.json");
    mkdirSync(join(engineDir, "pkg"), { recursive: true });
    mkdirSync(luarsSrcDir, { recursive: true });
    touch(pkgManifest, 1_000);
    touch(join(luarsSrcDir, "lua_vm.rs"), 2_000);
    // `../luars/src`, как в ENGINE_SOURCE_PATHS: путь от engineDir до соседнего каталога luars.
    expect(isEnginePkgStale(pkgManifest, [join(engineDir, "../luars/src")])).toBe(true);
  });

  it("исходники отсутствуют (стадия контейнерной сборки без cargo) — не устарел", () => {
    workDir = mkdtempSync(join(tmpdir(), "ensure-engine-pkg-"));
    const pkgManifest = join(workDir, "pkg", "package.json");
    mkdirSync(join(workDir, "pkg"), { recursive: true });
    touch(pkgManifest, 1_000);
    expect(
      isEnginePkgStale(pkgManifest, [
        join(workDir, "src"),
        join(workDir, "shaders"),
        join(workDir, "Cargo.toml"),
        join(workDir, "Cargo.lock"),
      ]),
    ).toBe(false);
  });
});
