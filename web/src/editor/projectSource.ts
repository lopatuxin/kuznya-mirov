import { createHttpProjectFileReader, type ProjectFileReader } from "../projectLoader";
import { createFolderProjectFileReader } from "./folderProjectFileReader";

/** Проект из списка `games/` или папка, открытая `showDirectoryPicker` — «Редактор», требования 4–7. */
export type ProjectSource =
  | { kind: "listed"; name: string }
  | { kind: "folder"; handle: FileSystemDirectoryHandle; displayName: string };

type ListedProjectSource = Extract<ProjectSource, { kind: "listed" }>;

/** Адрес, откуда проект из списка читает свои файлы — тот же, что раздаёт их и опросу («Редактор», требование 32). */
export function listedProjectBaseUrl(source: ListedProjectSource): string {
  return `/games/${source.name}/`;
}

export function createReaderForSource(source: ProjectSource): ProjectFileReader {
  return source.kind === "listed"
    ? createHttpProjectFileReader(listedProjectBaseUrl(source))
    : createFolderProjectFileReader(source.handle);
}

export function fallbackDisplayName(source: ProjectSource): string {
  return source.kind === "listed" ? source.name : source.displayName;
}
