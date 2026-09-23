import type { ProjectFileReader } from "../projectLoader";

/**
 * Путь в папке с диска разбирается по частям через `/` — «Редактор», требование 13. `..`, пустая
 * часть (двойной `/`, ведущий или хвостовой `/`) или абсолютный путь — всё это недопустимо и
 * читается как отсутствующий файл, а не роняет чтение.
 */
export function resolveFolderPathSegments(relativePath: string): string[] | null {
  if (relativePath.startsWith("/")) return null;
  const segments = relativePath.split("/");
  if (segments.some((segment) => segment === "" || segment === "..")) return null;
  return segments;
}

async function resolveFileHandle(
  root: FileSystemDirectoryHandle,
  relativePath: string,
): Promise<FileSystemFileHandle | null> {
  const segments = resolveFolderPathSegments(relativePath);
  if (segments === null) return null;

  const fileName = segments.at(-1);
  if (fileName === undefined) return null;

  let directory = root;
  for (const segment of segments.slice(0, -1)) {
    try {
      directory = await directory.getDirectoryHandle(segment);
    } catch {
      return null;
    }
  }

  try {
    return await directory.getFileHandle(fileName);
  } catch {
    return null;
  }
}

/**
 * Читалка файлов проекта из папки, открытой `showDirectoryPicker` — «Редактор», требования 7 и 12.
 */
export function createFolderProjectFileReader(root: FileSystemDirectoryHandle): ProjectFileReader {
  return {
    async readText(relativePath) {
      const handle = await resolveFileHandle(root, relativePath);
      if (handle === null) return null;
      try {
        return await (await handle.getFile()).text();
      } catch {
        return null;
      }
    },
    async readBinary(relativePath) {
      const handle = await resolveFileHandle(root, relativePath);
      if (handle === null) return null;
      try {
        return new Uint8Array(await (await handle.getFile()).arrayBuffer());
      } catch {
        return null;
      }
    },
  };
}
