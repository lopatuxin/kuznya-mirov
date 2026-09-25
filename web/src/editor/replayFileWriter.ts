import type { FileWriteResult } from "./projectFileWriter";
import { writeProjectFile } from "./projectFileWriter";
import { listedProjectBaseUrl, type ProjectSource } from "./projectSource";
import { formatReplayTimestamp, pickAvailableReplayFileName } from "./replayFilename";

/**
 * Файл существует? — «Редактор», требование 34 (суффикс `-2`). Проект из списка — `HEAD` без
 * кэша, как опрос («Редактор», требование 32); папка с диска — сам дескриптор `replays/`, которой
 * при самой первой записи ещё может не быть вовсе (тогда файла в ней точно нет).
 */
async function listedReplayExists(source: Extract<ProjectSource, { kind: "listed" }>, fileName: string): Promise<boolean> {
  try {
    const response = await fetch(`${listedProjectBaseUrl(source)}replays/${fileName}`, { method: "HEAD", cache: "no-store" });
    return response.ok;
  } catch {
    return false;
  }
}

async function folderReplayExists(root: FileSystemDirectoryHandle, fileName: string): Promise<boolean> {
  try {
    const replaysDir = await root.getDirectoryHandle("replays");
    await replaysDir.getFileHandle(fileName);
    return true;
  } catch {
    return false;
  }
}

function replayFileExists(source: ProjectSource, fileName: string): Promise<boolean> {
  return source.kind === "listed" ? listedReplayExists(source, fileName) : folderReplayExists(source.handle, fileName);
}

/**
 * Пишет запись в `replays/<файл>.json` папки с диска — «Редактор», требование 34: своя ветка, не
 * `writeProjectFile`, потому что только здесь папка создаётся сама (`{ create: true }`), а для
 * остальных файлов проекта запись в несуществующую папку остаётся отказом (фаза 09).
 */
async function writeReplayToFolder(root: FileSystemDirectoryHandle, fileName: string, text: string): Promise<FileWriteResult> {
  try {
    const replaysDir = await root.getDirectoryHandle("replays", { create: true });
    const fileHandle = await replaysDir.getFileHandle(fileName, { create: true });
    const writable = await fileHandle.createWritable();
    await writable.write(text);
    await writable.close();
    return { ok: true };
  } catch (error) {
    return { ok: false, reason: error instanceof Error ? error.message : String(error) };
  }
}

function writeReplayFile(source: ProjectSource, fileName: string, text: string): Promise<FileWriteResult> {
  return source.kind === "listed" ? writeProjectFile(source, `replays/${fileName}`, text) : writeReplayToFolder(source.handle, fileName, text);
}

export type SaveRecordingResult = { ok: true; fileName: string } | { ok: false; reason: string };

/**
 * «Сохранить запись» — «Редактор», требование 34: имя из времени начала партии, суффикс `-2` при
 * столкновении, запись `PUT` для проекта из списка или через дескриптор `replays/` папки с диска.
 */
export async function saveRecordingToProject(source: ProjectSource, startedAt: Date, recordingText: string): Promise<SaveRecordingResult> {
  const timestamp = formatReplayTimestamp(startedAt);
  const fileName = await pickAvailableReplayFileName(timestamp, (candidate) => replayFileExists(source, candidate));
  const result = await writeReplayFile(source, fileName, recordingText);
  return result.ok ? { ok: true, fileName } : { ok: false, reason: result.reason };
}
