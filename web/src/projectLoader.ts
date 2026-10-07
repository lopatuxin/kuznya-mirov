import type { Engine } from "engine";
import { GAME_FILE_FETCH, fetchText } from "./gameOptions";
import { buildImagePayload, fetchImageBytes, type ImageEntry, type ImageFileEntry } from "./images/imagePayload";
import { probeMusicVerdict, type MusicVerdict } from "./sound/soundLoader";
import type { EngineError } from "./engineErrors";

const RGBA_CHANNELS = 4;

type FontEntry = { name: string; path: string };
type SoundEntry = { index: number; name: string; path: string };
type MusicEntry = { index: number; name: string; path: string };
type TableEntry = { name: string; path: string };
type StampEntry = { name: string; path: string };

type ReadEntryFiles = {
  properties: string;
  scene: string;
  rules: string;
  screens: string;
  fonts: FontEntry[];
  tables: TableEntry[];
  stamps: StampEntry[];
  code?: string;
  terrain?: string;
  particles?: string;
};

type ReadEntryResult =
  | { ok: true; files: ReadEntryFiles; warnings: EngineError[] }
  | { ok: false; errors: EngineError[]; warnings: EngineError[] };

type ReadTextsResult = {
  fonts: FontEntry[];
  sounds: SoundEntry[];
  music: MusicEntry[];
  images: ImageEntry[];
  materials: ImageFileEntry[];
  masks: ImageFileEntry[];
};

type LoadResult =
  | { ok: true; warnings: EngineError[] }
  | { ok: false; errors: EngineError[]; warnings: EngineError[] };

type LoadedFont = { name: string; bytes: Uint8Array | null };
type LoadedTable = { name: string; text: string | null };
/** Текст файла штампа отпечатка; `null` — файл не найден, движок назовёт ошибку («Лепка рельефа»). */
export type LoadedStamp = { name: string; text: string | null };
export type LoadedSound = { index: number; path: string; bytes: Uint8Array | null };
type LoadedMusic = { index: number; path: string; bytes: Uint8Array | null };
/** Байты трека и приговор ему разом — «Звук» → «Два вида звука»: кто хочет проигрывать музыку
 *  (только страница игры, не редактор), решает по приговору, кому строить Blob-адрес сам. */
export type LoadedMusicVerdict = LoadedMusic & { verdict: MusicVerdict };
type MusicVerdictEntry = { index: number; verdict: MusicVerdict };

/** Маска покрытия, как её прочитала страница: путь файла и байт на точку — красный канал картинки, как его читает движок. */
export type LoadedCoverMask = { path: string; width: number; height: number; pixels: Uint8Array };

/**
 * Картинка `files.images`, как её разжала страница: размер и точки RGBA. Редактор рисует по ним уменьшенные кадры и
 * считает размер нового объекта; не прочитанных и не разжатых картинок здесь нет.
 */
export type LoadedProjectImage = { name: string; width: number; height: number; pixels: Uint8Array };

/** Подмножество `Engine`, которого хватает трём заходам загрузки — тестам достаточно подделать эти три метода. */
export type ProjectLoadEngine = Pick<Engine, "read_entry" | "read_texts" | "load">;

/**
 * Чтение одного файла проекта по пути относительно его корня («Редактор», требование 12). У
 * страницы игры и проекта из списка путь идёт в `fetch` от `/games/<имя>/`, у папки с диска — через
 * её дескриптор; сам модуль загрузки об этом различии не знает.
 */
export type ProjectFileReader = {
  readText(relativePath: string): Promise<string | null>;
  readBinary(relativePath: string): Promise<Uint8Array | null>;
};

export type ProjectLoadResult =
  | {
      status: "ok";
      warnings: EngineError[];
      gameJsonText: string;
      sceneText: string | null;
      loadedSounds: LoadedSound[];
      musicTracks: LoadedMusicVerdict[];
      /** Штампы `files.stamps` в порядке объявления с прочитанными текстами — редактор берёт из них пропорции нового отпечатка. */
      stamps: LoadedStamp[];
      /** Маски `covers` рельефа, что страница разжала для движка, — редактор красит по ним («Покраска»); не прочитанные или не разжатые не входят. */
      coverMasks: LoadedCoverMask[];
      /** Разжатые картинки `files.images` в порядке, в каком их назвал движок. */
      images: LoadedProjectImage[];
      audioContext: AudioContext;
    }
  /** `game.json` не читается вовсе — вызывающая сторона сама знает, как это назвать (имя игры или проекта). */
  | { status: "entry-missing" }
  | { status: "rejected"; errors: EngineError[]; warnings: EngineError[]; gameJsonText: string; sceneText: string | null };

/** Красный канал точек RGBA — значение маски покрытия, как его читает движок. */
function redChannelOf(rgba: Uint8Array): Uint8Array {
  const red = new Uint8Array(rgba.length / RGBA_CHANNELS);
  for (let point = 0; point < red.length; point += 1) red[point] = rgba[point * RGBA_CHANNELS] as number;
  return red;
}

async function fetchFonts(reader: ProjectFileReader, fonts: FontEntry[]): Promise<LoadedFont[]> {
  const bytesList = await Promise.all(fonts.map((font) => reader.readBinary(font.path)));
  return fonts.map((font, index) => ({ name: font.name, bytes: bytesList[index] ?? null }));
}

/** Таблицы данных читаются текстом во втором заходе, вместе с остальными файлами игры («Таблицы данных», требование 25). */
async function fetchTableTexts(reader: ProjectFileReader, tables: TableEntry[]): Promise<LoadedTable[]> {
  const textsList = await Promise.all(tables.map((table) => reader.readText(table.path)));
  return tables.map((table, index) => ({ name: table.name, text: textsList[index] ?? null }));
}

/** Штампы отпечатков читаются текстом во втором заходе, как таблицы («Лепка рельефа», требование 14). */
async function fetchStampTexts(reader: ProjectFileReader, stamps: StampEntry[]): Promise<LoadedStamp[]> {
  const textsList = await Promise.all(stamps.map((stamp) => reader.readText(stamp.path)));
  return stamps.map((stamp, index) => ({ name: stamp.name, text: textsList[index] ?? null }));
}

async function fetchSoundBytes(reader: ProjectFileReader, sounds: SoundEntry[]): Promise<LoadedSound[]> {
  const bytesList = await Promise.all(sounds.map((sound) => reader.readBinary(sound.path)));
  return sounds.map((sound, index) => ({ index: sound.index, path: sound.path, bytes: bytesList[index] ?? null }));
}

async function fetchMusicBytes(reader: ProjectFileReader, music: MusicEntry[]): Promise<LoadedMusic[]> {
  const bytesList = await Promise.all(music.map((track) => reader.readBinary(track.path)));
  return music.map((track, index) => ({ index: track.index, path: track.path, bytes: bytesList[index] ?? null }));
}

/**
 * Приговор исполнителя каждому читаемому треку — «Звук» → «Загрузка и проверка»: движок байтов
 * трека не держит вовсе, а по MP3 отвечает браузер. Blob-адрес для проигрывания сюда не входит —
 * его строит только страница игры, у которой звук вообще играет («Редактор» не проигрывает звук).
 */
async function readMusicVerdicts(audioContext: AudioContext, loaded: LoadedMusic[]): Promise<LoadedMusicVerdict[]> {
  const verdicts = await Promise.all(loaded.map((track) => probeMusicVerdict(audioContext, track.bytes)));
  return loaded.map((track, index) => ({ ...track, verdict: verdicts[index] as MusicVerdict }));
}

/**
 * Общий для страницы игры и редактора код трёх заходов загрузки — «Редактор», требования 11–12:
 * `read_entry` → `read_texts` → `load`, со шрифтами, звуками, приговорами музыке и картинками.
 * `gameJsonText` читает вызывающая сторона сама, до этого вызова, — ей нужно решить, что делать при
 * недоступном `game.json`, ещё до того, как заводить движок GPU («Редактор», требование про
 * отсутствующий game.json). `createAudioContext` зовётся один раз, сразу после `read_texts` — тем
 * же местом в порядке загрузки, что и раньше: приговор треку нужен даже там, где звук не играет
 * (редактор), иначе ошибки разошлись бы со страницей игры.
 */
export async function loadProject(
  engine: ProjectLoadEngine,
  reader: ProjectFileReader,
  gameJsonText: string,
  createAudioContext: () => AudioContext,
): Promise<Exclude<ProjectLoadResult, { status: "entry-missing" }>> {
  const entryResult = engine.read_entry(gameJsonText) as ReadEntryResult;
  if (!entryResult.ok) {
    return { status: "rejected", errors: entryResult.errors, warnings: entryResult.warnings, gameJsonText, sceneText: null };
  }

  const [propertiesText, sceneText, rulesText, screensText, codeText, tableTexts, terrainText, stamps, particlesText] = await Promise.all([
    reader.readText(entryResult.files.properties),
    reader.readText(entryResult.files.scene),
    reader.readText(entryResult.files.rules),
    reader.readText(entryResult.files.screens),
    entryResult.files.code !== undefined ? reader.readText(entryResult.files.code) : Promise.resolve(null),
    fetchTableTexts(reader, entryResult.files.tables),
    // «Рельеф», требование 47: файл высот читается вместе с остальными; без `files.terrain` движку уходит
    // `undefined`, а не `null` — `null` он читает как «файл назван, но не найден».
    entryResult.files.terrain !== undefined ? reader.readText(entryResult.files.terrain) : Promise.resolve(undefined),
    fetchStampTexts(reader, entryResult.files.stamps),
    // «Ветер и частицы», «Проверка перед запуском»: виды частиц читаются тем же заходом; без `files.particles` движку уходит `undefined`.
    entryResult.files.particles !== undefined ? reader.readText(entryResult.files.particles) : Promise.resolve(undefined),
  ]);

  const needed = engine.read_texts(propertiesText, sceneText, rulesText, screensText, codeText, terrainText) as ReadTextsResult;

  const audioContext = createAudioContext();

  const [fonts, loadedSounds, loadedMusic, loadedImages, loadedMaterials, loadedMasks] = await Promise.all([
    fetchFonts(reader, needed.fonts),
    fetchSoundBytes(reader, needed.sounds),
    fetchMusicBytes(reader, needed.music),
    fetchImageBytes(needed.images, reader.readBinary),
    fetchImageBytes(needed.materials, reader.readBinary),
    fetchImageBytes(needed.masks, reader.readBinary),
  ]);
  const [musicTracks, imagesPayload, materialMapsPayload, coverMasksPayload] = await Promise.all([
    readMusicVerdicts(audioContext, loadedMusic),
    buildImagePayload(loadedImages),
    buildImagePayload(loadedMaterials),
    buildImagePayload(loadedMasks),
  ]);
  const musicPayload: MusicVerdictEntry[] = musicTracks.map((track) => ({ index: track.index, verdict: track.verdict }));

  const loadResult = engine.load(
    propertiesText,
    sceneText,
    rulesText,
    screensText,
    fonts,
    loadedSounds,
    musicPayload,
    imagesPayload,
    codeText,
    tableTexts,
    terrainText,
    materialMapsPayload,
    coverMasksPayload,
    stamps,
    particlesText,
  ) as LoadResult;
  // read_entry первым, load вторым — тот же порядок, в котором предупреждения собирает сам движок
  // при объединённой загрузке («Редактор», требование 15).
  const warnings = [...entryResult.warnings, ...loadResult.warnings];
  if (!loadResult.ok) {
    return { status: "rejected", errors: loadResult.errors, warnings, gameJsonText, sceneText };
  }

  const coverMasks = loadedMasks.flatMap((image, index): LoadedCoverMask[] => {
    const decoded = coverMasksPayload[index];
    if (decoded?.verdict !== "ok") return [];
    return [{ path: image.path, width: decoded.width, height: decoded.height, pixels: redChannelOf(decoded.pixels) }];
  });
  const images = needed.images.flatMap((entry, position): LoadedProjectImage[] => {
    const decoded = imagesPayload[position];
    return decoded?.verdict === "ok" ? [{ name: entry.name, width: decoded.width, height: decoded.height, pixels: decoded.pixels }] : [];
  });
  return { status: "ok", warnings, gameJsonText, sceneText, loadedSounds, musicTracks, stamps, coverMasks, images, audioContext };
}

/**
 * Читалка файлов проекта по HTTP — страница игры и проекты из списка `games/`, оба по одному и тому
 * же адресу `/games/<имя>/…`.
 */
export function createHttpProjectFileReader(baseUrl: string): ProjectFileReader {
  return {
    readText: (relativePath) => fetchText(`${baseUrl}${relativePath}`),
    readBinary: async (relativePath) => {
      try {
        const response = await fetch(`${baseUrl}${relativePath}`, GAME_FILE_FETCH);
        return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
      } catch {
        return null;
      }
    },
  };
}
