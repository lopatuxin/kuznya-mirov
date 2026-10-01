// Вырезает штампы гор из плиток высот Terrain Tiles и пишет их в папку игры. Одни и те же плитки и
// список всегда дают одни и те же файлы.
// Запуск: `node tools/stamps/cut.mjs art/rpg/stamps.json`.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { readCutList } from "./list.mjs";
import { cutStamp, stampText } from "./stamp.mjs";
import { createTileReader } from "./tiles.mjs";

/** Тексты файлов штампов по вырезам из `readCutList`: `[{ name, text }]`. Ничего не пишет, ошибка любого выреза останавливает всё. */
async function cutStamps(cuts, readTile) {
  const stamps = [];
  for (const cut of cuts) {
    try {
      stamps.push({ name: cut.name, text: stampText(await cutStamp(cut, readTile)) });
    } catch (error) {
      throw new Error(`вырез «${cut.name}»: ${error.message}`);
    }
  }
  return stamps;
}

/** Режет штампы списка из файла `listPath` и пишет их в папку `out` списка. Список или плитка с ошибкой — ни одного файла. */
export async function cutFromFile(listPath, readTile) {
  let list;
  try {
    list = JSON.parse(await readFile(listPath, "utf8"));
  } catch (error) {
    throw new Error(`${listPath}: ${error.message}`);
  }
  const { out, cuts } = readCutList(list);
  const stamps = await cutStamps(cuts, readTile);
  const folder = resolve(dirname(listPath), out);
  await mkdir(folder, { recursive: true });
  const files = [];
  for (const { name, text } of stamps) {
    const file = join(folder, `${name}.json`);
    await writeFile(file, text);
    files.push(file);
  }
  return files;
}

// Тест импортирует функции этого файла, и импорт не должен запускать вырезку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [listPath] = process.argv.slice(2);
  if (!listPath) {
    console.error("node tools/stamps/cut.mjs <список.json>");
    process.exit(1);
  }
  cutFromFile(listPath, createTileReader())
    .then((files) => files.forEach((file) => console.log(`штамп → ${file}`)))
    .catch((error) => {
      console.error(error.message);
      process.exit(1);
    });
}
