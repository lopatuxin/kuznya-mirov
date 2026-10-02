import { randomBytes } from "node:crypto";
import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { StrokeWrite } from "../src/editor/strokeRunner";

/** «Мазки командой», требование 22: относительный путь считается от папки, где запущена команда; `npm run` сам переходит в `web/`, а прежнюю папку отдаёт в `INIT_CWD`. */
export function resolveCommandPath(argument: string, env: NodeJS.ProcessEnv, cwd: string): string {
  return resolve(env.INIT_CWD ?? cwd, argument);
}

/** Файл появляется целиком или не появляется: открытый редактор опрашивает проект и не должен прочесть половину PNG. */
function writeFileAtomically(target: string, content: string | Uint8Array): void {
  mkdirSync(dirname(target), { recursive: true });
  const tempPath = `${target}.tmp-${randomBytes(6).toString("hex")}`;
  writeFileSync(tempPath, content);
  renameSync(tempPath, target);
}

/** Файлы пишутся в порядке списка — маски, рельеф, `game.json` — каждый атомарно. */
export function writeStrokeFiles(projectFolder: string, writes: readonly StrokeWrite[]): void {
  for (const write of writes) writeFileAtomically(join(projectFolder, write.path), write.content);
}
