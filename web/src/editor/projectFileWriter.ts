import { resolveFolderPathSegments } from "./folderProjectFileReader";
import { listedProjectBaseUrl, type ProjectSource } from "./projectSource";

export type FileWriteResult = { ok: true } | { ok: false; reason: string };

const JSON_CONTENT_TYPE = "application/json";

/**
 * Запись файла проекта из списка — «Редактор», требования 3–4: `PUT /games/<имя>/<путь>` с
 * текстом файла в теле; на стенде его принимает nginx, в `npm run dev` — раздача `games/` в
 * `web/vite.config.ts`. Ответ вне 2xx или сетевая ошибка — причина строкой для верхней полосы.
 */
async function writeListedFile(source: Extract<ProjectSource, { kind: "listed" }>, relativePath: string, text: string): Promise<FileWriteResult> {
  let response: Response;
  try {
    response = await fetch(`${listedProjectBaseUrl(source)}${relativePath}`, {
      method: "PUT",
      headers: { "Content-Type": JSON_CONTENT_TYPE },
      body: text,
    });
  } catch (error) {
    return { ok: false, reason: error instanceof Error ? error.message : String(error) };
  }
  return response.ok ? { ok: true } : { ok: false, reason: `сервер отказал в записи (код ${response.status})` };
}

/**
 * Запись файла в папку с диска через её дескриптор — «Редактор», требования 3–4, 26:
 * `createWritable()` уже открытого разрешения на запись. Отозванное разрешение и любая другая
 * ошибка записи гасятся в текст причины, а не бросаются наружу.
 */
async function writeFolderFile(root: FileSystemDirectoryHandle, relativePath: string, text: string): Promise<FileWriteResult> {
  const segments = resolveFolderPathSegments(relativePath);
  const fileName = segments?.at(-1);
  if (segments === null || fileName === undefined) {
    return { ok: false, reason: `недопустимый путь «${relativePath}»` };
  }
  try {
    let directory = root;
    for (const segment of segments.slice(0, -1)) {
      directory = await directory.getDirectoryHandle(segment);
    }
    const fileHandle = await directory.getFileHandle(fileName);
    const writable = await fileHandle.createWritable();
    await writable.write(text);
    await writable.close();
    return { ok: true };
  } catch (error) {
    return { ok: false, reason: error instanceof Error ? error.message : String(error) };
  }
}

/** Пишет файл проекта по его пути от корня — «Редактор», требование 3: сама запись зависит от источника проекта. */
export function writeProjectFile(source: ProjectSource, relativePath: string, text: string): Promise<FileWriteResult> {
  return source.kind === "listed" ? writeListedFile(source, relativePath, text) : writeFolderFile(source.handle, relativePath, text);
}
