import { existsSync, readdirSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { join, resolve } from "node:path";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const engineDir = resolve(scriptDir, "../../engine");
const enginePkgManifest = resolve(engineDir, "pkg/package.json");

// Только исходники движка, не весь `engine/` — иначе `engine/pkg` сам оказался бы под
// наблюдением, и каждая пересборка тут же считалась бы новым изменением, требующим пересборки.
// `luars` — своя копия библиотеки, от которой зависит `engine/Cargo.toml`: правка её исходников
// тоже должна делать `pkg` устаревшим, как и правка самого движка.
const ENGINE_SOURCE_PATHS = [
  "src",
  "shaders",
  "Cargo.toml",
  "Cargo.lock",
  "../luars/src",
  "../luars/Cargo.toml",
  "../luars/Cargo.lock",
].map((entry) => resolve(engineDir, entry));

/**
 * Самое позднее время правки файла или дерева файлов по `path`. Отсутствующий путь (например,
 * `engine/shaders` в стадии контейнерной сборки, где скопирован только готовый `engine/pkg`, без
 * исходников) не ошибка — он просто не даёт вклада в максимум, а не останавливает проверку.
 */
export function latestMtimeMs(path) {
  if (!existsSync(path)) return -Infinity;
  const stats = statSync(path);
  if (!stats.isDirectory()) return stats.mtimeMs;

  let latest = stats.mtimeMs;
  for (const entry of readdirSync(path, { recursive: true })) {
    const entryMtimeMs = statSync(join(path, entry)).mtimeMs;
    if (entryMtimeMs > latest) latest = entryMtimeMs;
  }
  return latest;
}

// Аргументы по умолчанию — реальный `engine/pkg` и его исходники; тесты передают свои временные
// пути вместо них, не трогая настоящий движок.
export function isEnginePkgStale(pkgManifestPath = enginePkgManifest, sourcePaths = ENGINE_SOURCE_PATHS) {
  if (!existsSync(pkgManifestPath)) return true;
  const pkgBuiltAtMs = statSync(pkgManifestPath).mtimeMs;
  const sourceChangedAtMs = Math.max(...sourcePaths.map(latestMtimeMs));
  return sourceChangedAtMs > pkgBuiltAtMs;
}

/**
 * `web/package.json` зависит от `engine/pkg` (`file:../engine/pkg`), но `engine/.gitignore`
 * исключает `pkg` из репозитория — на чистом клоне его нет, и `npm install`/`vite` падают без
 * понятной причины. Полная пересборка (`wasm-pack`, компиляция Rust в WebAssembly) дорога, поэтому
 * запускается только когда `pkg` отсутствует или исходники движка новее уже собранного `pkg` —
 * следующий `npm run dev`/`build` после правки Rust-кода подхватывает её сам, без ручного
 * удаления `pkg`.
 */
// Модуль импортируется тестами напрямую (см. ensureEnginePkg.test.mjs) — без этой проверки любой
// такой импорт тут же запускал бы настоящую сборку движка или завершал бы процесс тестов.
const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  if (!isEnginePkgStale()) {
    process.exit(0);
  }

  console.log("engine/pkg отсутствует или устарел — собираю движок (wasm-pack build --target web --out-dir pkg)...");

  const result = spawnSync("wasm-pack", ["build", "--target", "web", "--out-dir", "pkg"], {
    cwd: engineDir,
    stdio: "inherit",
    shell: false,
  });

  if (result.error) {
    console.error(
      "Не удалось запустить wasm-pack: " +
        result.error.message +
        "\nУстановите wasm-pack (https://rustwasm.github.io/wasm-pack/) и повторите.",
    );
    process.exit(1);
  }

  process.exit(result.status ?? 1);
}
