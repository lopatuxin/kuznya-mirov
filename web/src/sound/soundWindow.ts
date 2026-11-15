// «Звук» → «Окно чисел»: разметка ровно та, что пишет
// `engine::core::sound::SoundWindow` — [0] звук включён (0/1), [1] номер нужного трека или −1,
// [2] громкость музыки 0–100, [3] громкость звуков 0–100, [4..] отметки по номерам звуков. Пишет окно только движок, страница только читает.

const ENABLED_INDEX = 0;
const MUSIC_INDEX = 1;
const MUSIC_VOLUME_INDEX = 2;
const SOUND_VOLUME_INDEX = 3;
const HEADER_LEN = 4;

export type SoundWindowSnapshot = {
  enabled: boolean;
  musicId: number | null;
  musicVolume: number;
  soundVolume: number;
  raisedSoundIds: number[];
};

const EMPTY_SNAPSHOT: SoundWindowSnapshot = { enabled: false, musicId: null, musicVolume: 100, soundVolume: 100, raisedSoundIds: [] };

/**
 * Читает окно чисел движка из памяти wasm. Вид — свежий `Int32Array` поверх `memory.buffer` при
 * каждом вызове, а не закэшированный: рост памяти движка отвязывает старые представления от живого
 * буфера, а окно читается ровно так, как того требует договор — «после каждого `tick()`».
 */
export function readSoundWindow(memory: WebAssembly.Memory, ptr: number, len: number): SoundWindowSnapshot {
  if (ptr === 0 || len < HEADER_LEN) return EMPTY_SNAPSHOT;

  const view = new Int32Array(memory.buffer, ptr, len);
  const enabled = view[ENABLED_INDEX] !== 0;
  const rawMusic = view[MUSIC_INDEX] as number;
  const musicId = rawMusic >= 0 ? rawMusic : null;

  const raisedSoundIds: number[] = [];
  for (let i = HEADER_LEN; i < len; i++) {
    if (view[i] !== 0) raisedSoundIds.push(i - HEADER_LEN);
  }

  return {
    enabled,
    musicId,
    musicVolume: view[MUSIC_VOLUME_INDEX] as number,
    soundVolume: view[SOUND_VOLUME_INDEX] as number,
    raisedSoundIds,
  };
}
