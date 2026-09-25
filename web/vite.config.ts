import { randomBytes } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { rename, writeFile } from "node:fs/promises";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import type { IncomingMessage } from "node:http";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const rootDir = fileURLToPath(new URL(".", import.meta.url));
const gamesDir = resolve(rootDir, "../games");

const CONTENT_TYPES: Record<string, string> = {
  ".json": "application/json; charset=utf-8",
};

/**
 * `candidatePath` резолвлен (`resolve`/`join`), поэтому сравнение строковое: любой `..` в исходном
 * пути уже схлопнут, а совпадение по префиксу с добавлением `sep` отсекает и точное совпадение с
 * `directoryPath`, и случайное совпадение имени вроде `games` против `gamesX`.
 */
export function isPathWithinDirectory(candidatePath: string, directoryPath: string): boolean {
  return candidatePath === directoryPath || candidatePath.startsWith(directoryPath + sep);
}

/**
 * `requestUrl` приходит от клиента без валидации: битая процентная кодировка (`%ZZ`) роняет
 * `decodeURIComponent`, а путь с нулевым байтом роняет `statSync` (`ERR_INVALID_ARG_VALUE`).
 * Оба случая — невалидный запрос, а не сбой сервера, поэтому здесь они гасятся в `null`, а
 * вызывающий код сам отвечает на него 404 (см. комментарий у `configureServer` ниже).
 */
export function resolveGameFilePath(requestUrl: string, directoryPath: string): string | null {
  let requestPath: string;
  try {
    requestPath = decodeURIComponent(requestUrl.split("?")[0] ?? "");
  } catch {
    return null;
  }

  const filePath = resolve(join(directoryPath, requestPath));
  if (!isPathWithinDirectory(filePath, directoryPath)) {
    return null;
  }

  try {
    return statSync(filePath).isFile() ? filePath : null;
  } catch {
    return null;
  }
}

export type GamesFileResponse = {
  headers: Record<string, string>;
  /** `null` — ответ на `HEAD`, без тела. */
  body: Buffer | null;
};

/**
 * Ответ на запрос файла из `games/` — заголовки `ETag` и `Last-Modified` по размеру и времени
 * изменения файла, тело только на `GET` («Редактор», требование 32: опрос ходит `HEAD`-запросами и
 * так находит правку без пересборки образа). `filePath` — уже проверенный существующий файл
 * (`resolveGameFilePath`); стат может всё равно упасть, если файл исчез между проверкой и этим
 * вызовом — вызывающий код гасит это в 404 так же, как отсутствие файла с самого начала.
 */
export function buildGamesFileResponse(filePath: string, method: string): GamesFileResponse {
  const stats = statSync(filePath);
  const headers: Record<string, string> = {
    "Content-Type": CONTENT_TYPES[extname(filePath)] ?? "application/octet-stream",
    "Content-Length": String(stats.size),
    ETag: `"${stats.size.toString(16)}-${stats.mtimeMs.toString(16)}"`,
    "Last-Modified": stats.mtime.toUTCString(),
  };
  if (method === "HEAD") return { headers, body: null };

  // Файл может измениться между этим statSync и чтением ниже — Content-Length ответа берём из
  // длины уже прочитанного буфера, а не из более раннего stats.size, чтобы заголовок не разошёлся
  // с реально отданным телом.
  const body = readFileSync(filePath);
  headers["Content-Length"] = String(body.length);
  return { headers, body };
}

export type PutTargetResult =
  | { status: "forbidden" }
  | { status: "not-allowed" }
  | { status: "ok"; filePath: string; createDirectory: boolean };

/**
 * Проверка пути записи — «Редактор», требования 28 (фаза 09) и 34 (фаза 10): пишутся только
 * `.json` внутри `games/<имя>/`, на любой глубине — файлы из `files.scene`/`files.properties`
 * могут лежать в подпапке проекта (фаза 09), а `replays/` вдобавок разрешена самим этим кодом.
 * `..`, абсолютный путь и битая процентная кодировка сводятся к тому же выходу за `directoryPath`,
 * что и у чтения (`isPathWithinDirectory`), — 403; путь внутри каталога, но не `.json` или без
 * имени проекта — 405. Ни одна папка, кроме `replays/` проекта, сама не создаётся (фаза 09: «без
 * папки запись не делается») — `createDirectory` говорит `writeGamesFileAtomically`, можно ли
 * создать недостающую папку для этого пути; `replays/` несуществующего проекта — 403, а не тихое
 * создание заодно и его самого.
 */
export function resolvePutTarget(requestUrl: string, directoryPath: string): PutTargetResult {
  let requestPath: string;
  try {
    requestPath = decodeURIComponent(requestUrl.split("?")[0] ?? "");
  } catch {
    return { status: "forbidden" };
  }
  // Нулевой байт роняет запись файла так же, как statSync у чтения — тут его некому поймать заранее.
  if (requestPath.includes("\0")) return { status: "forbidden" };

  const filePath = resolve(join(directoryPath, requestPath));
  if (!isPathWithinDirectory(filePath, directoryPath) || filePath === directoryPath) {
    return { status: "forbidden" };
  }

  const relativeSegments = filePath.slice(directoryPath.length + 1).split(sep);
  const isProjectFile = relativeSegments.length >= 2 && relativeSegments.every((segment) => segment !== "");
  if (!isProjectFile || extname(filePath) !== ".json") {
    return { status: "not-allowed" };
  }

  const isReplayFile = relativeSegments.length === 3 && relativeSegments[1] === "replays";
  if (isReplayFile && !existsSync(join(directoryPath, relativeSegments[0] as string))) {
    return { status: "forbidden" };
  }
  return { status: "ok", filePath, createDirectory: isReplayFile };
}

/** Тело запроса целиком — раздача `games/` не подключает парсер тела, читает поток сама. */
function readRequestBody(req: IncomingMessage): Promise<Buffer> {
  return new Promise((resolvePromise, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => chunks.push(chunk));
    req.on("end", () => resolvePromise(Buffer.concat(chunks)));
    req.on("error", reject);
  });
}

/**
 * Пишет файл проекта во временный рядом и переименовывает — «Редактор», требование 29: наполовину
 * записанный файл не виден ни опросу редактора (своё имя, не `.json`), ни `rename`, который в POSIX
 * и NTFS атомарно подменяет старое содержимое новым. `createDirectory` — самая первая запись партии
 * («Редактор», требование 34): единственный случай, где недостающая папка (`replays/`) создаётся
 * сама, а не остаётся отказом, как у любого другого пути без папки (фаза 09).
 */
export async function writeGamesFileAtomically(filePath: string, body: Buffer, createDirectory: boolean): Promise<void> {
  if (createDirectory) mkdirSync(dirname(filePath), { recursive: true });
  const tempPath = `${filePath}.tmp-${randomBytes(6).toString("hex")}`;
  await writeFile(tempPath, body);
  await rename(tempPath, filePath);
}

/**
 * Игровые папки (`games/snake/`, `games/arkanoid/`) лежат рядом с web/, вне корня Vite. Плагин
 * раздаёт их по `/games/...` в dev-режиме и копирует в outDir при сборке, чтобы движок мог
 * загрузить игру по HTTP так же, как это будет работать после публикации сборки.
 */
function serveGamesFolder(): Plugin {
  let outDir = resolve(rootDir, "dist");

  return {
    name: "kuznya-serve-games-folder",
    configResolved(config) {
      // Vite сам резолвит build.outDir относительно config.root, а не относительно папки
      // vite.config.ts; повторяем ровно эту формулу, чтобы копия games/ гарантированно попадала
      // в тот же каталог, куда Vite пишет сборку, а не в случайно совпавший при root === rootDir.
      outDir = resolve(config.root, config.build.outDir);
    },
    configureServer(server) {
      server.middlewares.use("/games", (req, res) => {
        // «Редактор», требование 28: PUT пишет файл — своей веткой, до чтения ниже.
        if (req.method === "PUT") {
          const target = resolvePutTarget(req.url ?? "", gamesDir);
          if (target.status === "forbidden") {
            res.statusCode = 403;
            res.end();
            return;
          }
          if (target.status === "not-allowed") {
            res.statusCode = 405;
            res.end();
            return;
          }
          readRequestBody(req)
            .then((body) => writeGamesFileAtomically(target.filePath, body, target.createDirectory))
            .then(() => {
              res.statusCode = 204;
              res.end();
            })
            .catch(() => {
              res.statusCode = 500;
              res.end();
            });
          return;
        }

        // Отвечаем 404 сами, не через next(): за этим префиксом ничего, кроме игровых файлов, не
        // раздаётся, а next() на невалидном пути передал бы тот же необработанный %-эскейп штатному
        // статическому middleware Vite, который на нём падает своим decodeURI с 500.
        const filePath = resolveGameFilePath(req.url ?? "", gamesDir);
        if (filePath === null) {
          res.statusCode = 404;
          res.end();
          return;
        }
        // Между проверкой и чтением файл может исчезнуть или стать недоступным: такой запрос —
        // тот же самый «файла нет», что и не прошедший проверку путь, а не отказ сервера.
        let response: GamesFileResponse;
        try {
          response = buildGamesFileResponse(filePath, req.method ?? "GET");
        } catch {
          res.statusCode = 404;
          res.end();
          return;
        }
        for (const [name, value] of Object.entries(response.headers)) {
          res.setHeader(name, value);
        }
        res.end(response.body ?? undefined);
      });
    },
    // writeBundle, а не closeBundle: dev-сервер при остановке тоже прогоняет closeBundle через
    // свой контейнер плагинов (совместимость с Rollup-хуками), и копирование срабатывало бы на
    // каждом Ctrl+C, создавая dist/games на ровном месте. writeBundle сборкой output-бандла не
    // симулируется и во время dev не вызывается вовсе.
    writeBundle() {
      if (!existsSync(gamesDir)) {
        throw new Error(`Папка с играми не найдена: ${gamesDir}. Сборка остановлена.`);
      }
      mkdirSync(outDir, { recursive: true });
      cpSync(gamesDir, join(outDir, "games"), { recursive: true });
    },
  };
}

export default defineConfig({
  // Страница одна, своей маршрутизации нет: без этого Vite отдаёт index.html на любой
  // несуществующий путь (SPA-фолбэк), и страница не может отличить отсутствующий файл игры
  // от существующего по коду ответа.
  appType: "mpa",
  // React стоит только у редактора (`web/src/editor/**`); страница игры (`index.html`/`main.ts`)
  // на нём не написана, и плагин её сборку не трогает — babel обрабатывает только .tsx/.jsx.
  plugins: [serveGamesFolder(), react()],
  optimizeDeps: {
    // Пакет wasm-bindgen сам находит свой .wasm через import.meta.url; esbuild-предбандлинг
    // ломает этот путь, поэтому пакет исключён из dep-оптимизации Vite.
    exclude: ["engine"],
  },
  server: {
    fs: {
      // node_modules/engine — символьная ссылка на engine/pkg (file:-зависимость), которая лежит
      // вне корня web/; без явного разрешения Vite отдаёт её файлы с 403.
      allow: [rootDir, resolve(rootDir, "../engine/pkg")],
    },
  },
  build: {
    // Вторая страница сборки — «Редактор», требование 1: `/editor.html` на собранном сайте и в
    // контейнере, без изменений в nginx.
    rollupOptions: {
      input: {
        main: resolve(rootDir, "index.html"),
        editor: resolve(rootDir, "editor.html"),
      },
    },
  },
});
