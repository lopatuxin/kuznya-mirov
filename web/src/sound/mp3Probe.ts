// «Звуковые файлы» → «Проверка данных перед запуском»: браузер не берётся разжимать трек целиком
// ради проверки — ему хватает начала файла. Здесь только длина этого начала; сам вызов
// `decodeAudioData` и его результат («ok»/«rejected») — забота исполнителя, не чистой функции.

const ID3V2_MAGIC = [0x49, 0x44, 0x33]; // "ID3"
const PROBE_TAIL_BYTES = 64 * 1024;

/**
 * Размер заголовка ID3v2 в начале файла вместе с самим десятибайтным заголовком — 0, если тега нет.
 * Размер после заголовка записан в четырёх байтах по семь значащих бит (synchsafe: старший бит
 * каждого байта всегда 0), поэтому его нельзя читать как обычное 32-битное число.
 */
export function id3v2HeaderSize(bytes: Uint8Array): number {
  if (bytes.length < 10) return 0;
  if (bytes[0] !== ID3V2_MAGIC[0] || bytes[1] !== ID3V2_MAGIC[1] || bytes[2] !== ID3V2_MAGIC[2]) return 0;

  const size =
    (((bytes[6] as number) & 0x7f) << 21) |
    (((bytes[7] as number) & 0x7f) << 14) |
    (((bytes[8] as number) & 0x7f) << 7) |
    ((bytes[9] as number) & 0x7f);
  const hasFooter = ((bytes[5] as number) & 0x10) !== 0;
  return 10 + size + (hasFooter ? 10 : 0);
}

/**
 * Сколько байт с начала MP3-файла отдать `decodeAudioData` для проверки: ID3v2-заголовок целиком
 * (если он есть) плюс около 64 КБ звуковых данных — не длиннее самого файла.
 */
export function mp3ProbeLength(bytes: Uint8Array): number {
  return Math.min(bytes.length, id3v2HeaderSize(bytes) + PROBE_TAIL_BYTES);
}
