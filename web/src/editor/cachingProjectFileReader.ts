import type { ProjectFileReader } from "../projectLoader";

export type CachingProjectFileReader = {
  /** Читает с диска или по сети и запоминает прочитанное — для обычной (не по правке) загрузки. */
  reader: ProjectFileReader;
  /** Отдаёт только уже прочитанное, на диск и в сеть не ходит — «Редактор», требование 1. */
  cachedReader: ProjectFileReader;
  /** Текст, который в последний раз прочитала обычная загрузка, `undefined` — путь ещё не читался. */
  getCachedText(relativePath: string): string | null | undefined;
  /** Отмечает путь как прочитанный с данным текстом — своя запись становится новой «правдой диска». */
  setCachedText(relativePath: string, text: string): void;
  /** Сколько раз редактор записал файл — требование 24: метка «перезагрузка начата до записи N». */
  getWriteCount(): number;
};

/**
 * Обёртка читалки проекта, которая запоминает прочитанное по пути — «Редактор», требование 1:
 * действие проверяется загрузкой, где `game.json`, правила, экраны, код, шрифты, звуки, картинки
 * берутся из прочитанного последней (настоящей, не по правке) загрузкой и с диска не перечитываются.
 */
export function createCachingProjectFileReader(baseReader: ProjectFileReader): CachingProjectFileReader {
  const texts = new Map<string, string | null>();
  const binaries = new Map<string, Uint8Array | null>();
  let writeCount = 0;

  return {
    reader: {
      async readText(relativePath) {
        const value = await baseReader.readText(relativePath);
        texts.set(relativePath, value);
        return value;
      },
      async readBinary(relativePath) {
        const value = await baseReader.readBinary(relativePath);
        binaries.set(relativePath, value);
        return value;
      },
    },
    cachedReader: {
      readText: async (relativePath) => texts.get(relativePath) ?? null,
      readBinary: async (relativePath) => binaries.get(relativePath) ?? null,
    },
    getCachedText: (relativePath) => texts.get(relativePath),
    setCachedText: (relativePath, text) => {
      texts.set(relativePath, text);
      writeCount += 1;
    },
    getWriteCount: () => writeCount,
  };
}

/**
 * Читалка для загрузки по правке — «Редактор», требование 1: `scene.json`/`properties.json` идут
 * из переданных текстов правки, остальное — из кэша обычной загрузки, без сети и диска.
 */
export function createOverridingReader(cachedReader: ProjectFileReader, overrides: Readonly<Record<string, string>>): ProjectFileReader {
  return {
    readText: (relativePath) => (relativePath in overrides ? Promise.resolve(overrides[relativePath] as string) : cachedReader.readText(relativePath)),
    readBinary: (relativePath) => cachedReader.readBinary(relativePath),
  };
}
