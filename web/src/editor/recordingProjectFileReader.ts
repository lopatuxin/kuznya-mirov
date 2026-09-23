import type { ProjectFileReader } from "../projectLoader";

export type RecordingProjectFileReader = {
  reader: ProjectFileReader;
  /** Сбрасывает список путей перед новой загрузкой — «Редактор», требование 32. */
  reset(): void;
  /** Пути, которые прочитала загрузка с последнего `reset()`, в порядке первого чтения. */
  getReadPaths(): string[];
};

/**
 * Обёртка читалки проекта, которая запоминает пути, что читала последняя загрузка — ровно те, что
 * потом опрашивает `pollListedProject` («Редактор», требование 32).
 */
export function createRecordingProjectFileReader(reader: ProjectFileReader): RecordingProjectFileReader {
  let paths: string[] = [];

  function record(relativePath: string): void {
    if (!paths.includes(relativePath)) paths.push(relativePath);
  }

  return {
    reader: {
      async readText(relativePath) {
        record(relativePath);
        return reader.readText(relativePath);
      },
      async readBinary(relativePath) {
        record(relativePath);
        return reader.readBinary(relativePath);
      },
    },
    reset() {
      paths = [];
    },
    getReadPaths() {
      return paths;
    },
  };
}
