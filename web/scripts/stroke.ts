import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { initSync, terrain_readings } from "engine";
import { runStrokes, type TerrainReadingsFunction } from "../src/editor/strokeRunner";
import type { ProjectFileReader } from "../src/projectLoader";
import { resolveCommandPath, writeStrokeFiles } from "./strokeFiles";

/** `npm run stroke -- <папка проекта> <файл мазков>` — «Кисти», «Мазки командой»; пути считаются от папки, где запущена команда. */
const ENGINE_WASM_URL = new URL("../../engine/pkg/engine_bg.wasm", import.meta.url);

function readOrNull(path: string): Buffer | null {
  try {
    return readFileSync(path);
  } catch {
    return null;
  }
}

function createFolderReader(root: string): ProjectFileReader {
  return {
    readText: async (relativePath) => readOrNull(join(root, relativePath))?.toString("utf8") ?? null,
    readBinary: async (relativePath) => {
      const bytes = readOrNull(join(root, relativePath));
      return bytes === null ? null : new Uint8Array(bytes);
    },
  };
}

function fail(message: string): void {
  console.error(message);
  process.exitCode = 1;
}

async function main(): Promise<void> {
  const [projectArgument, strokeFileArgument] = process.argv.slice(2);
  if (projectArgument === undefined || strokeFileArgument === undefined) {
    fail("Нужны два аргумента: папка проекта и файл мазков — npm run stroke -- <папка проекта> <файл мазков>");
    return;
  }
  const projectFolder = resolveCommandPath(projectArgument, process.env, process.cwd());
  const strokeFile = readOrNull(resolveCommandPath(strokeFileArgument, process.env, process.cwd()));
  if (strokeFile === null) {
    fail(`${strokeFileArgument}: файла мазков нет`);
    return;
  }
  const engineBytes = readOrNull(fileURLToPath(ENGINE_WASM_URL));
  if (engineBytes === null) {
    fail("Нет engine/pkg: соберите движок командой node scripts/ensureEnginePkg.mjs");
    return;
  }
  initSync({ module: engineBytes });

  const result = await runStrokes(createFolderReader(projectFolder), terrain_readings as TerrainReadingsFunction, strokeFile.toString("utf8"));
  if (result.status === "error") {
    fail(result.message);
    return;
  }
  writeStrokeFiles(projectFolder, result.writes);
  console.log(result.message);
}

await main();
