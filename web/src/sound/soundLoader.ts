import { mp3ProbeLength } from "./mp3Probe";

export type MusicVerdict = "ok" | "missing" | "rejected";

/**
 * Кто выносит приговор файлу из `files.music`, решает исполнитель, а не движок — «Звук в данных
 * игры» → «Кто отвечает на вопрос «годен ли файл»»: движок байтов трека не держит вовсе. Пробуется
 * только начало файла — трек при проверке не распаковывается целиком (см. `mp3ProbeLength`).
 */
export async function probeMusicVerdict(
  audioContext: AudioContext,
  bytes: Uint8Array | null,
): Promise<MusicVerdict> {
  if (bytes === null) return "missing";
  const probe = bytes.slice(0, mp3ProbeLength(bytes));
  try {
    // `decodeAudioData` отбирает переданный буфер — `probe` уже своя копия (результат `slice`),
    // делить её с чем-либо ещё не нужно.
    await audioContext.decodeAudioData(probe.buffer as ArrayBuffer);
    return "ok";
  } catch {
    return "rejected";
  }
}

/**
 * Разжимает короткий звук в готовый `AudioBuffer` до старта кадрового цикла — «Звуковые файлы» →
 * «Два способа проигрывания»: звук события живёт в памяти страницы готовыми отсчётами. Копия байт
 * обязательна: `decodeAudioData` отбирает переданный буфер, а исходные байты того же звука дальше
 * никому не нужны.
 */
export async function decodeSoundBuffer(audioContext: AudioContext, bytes: Uint8Array): Promise<AudioBuffer> {
  const copy = bytes.slice();
  return audioContext.decodeAudioData(copy.buffer as ArrayBuffer);
}
