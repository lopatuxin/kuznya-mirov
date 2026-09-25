import type { LoadedMusicVerdict, LoadedSound } from "../projectLoader";
import { decodeSoundBuffer } from "../sound/soundLoader";
import type { MusicAsset, SoundAssets } from "../sound/soundPlayer";

/**
 * Партия в редакторе звучит теми же модулями `web/src/sound`, что и страница игры («Редактор»,
 * требование 1) — эти два небольших сборщика повторяют `decodeSoundAssets`/`buildMusicAssets`
 * страницы (`web/src/main.ts`), не трогая её файл: строятся только здесь, из тех же данных
 * `ProjectLoadResult`, которые загрузка уже приготовила и для страницы, и для редактора.
 */
export async function decodeBattleSoundBuffers(audioContext: AudioContext, sounds: LoadedSound[]): Promise<Map<number, AudioBuffer>> {
  const buffers = new Map<number, AudioBuffer>();
  await Promise.all(
    sounds.map(async (sound) => {
      if (!sound.bytes) return;
      try {
        buffers.set(sound.index, await decodeSoundBuffer(audioContext, sound.bytes));
      } catch {
        console.warn(`исполнитель: ${sound.path} — не разжимается`);
      }
    }),
  );
  return buffers;
}

export function buildBattleMusicAssets(tracks: LoadedMusicVerdict[]): Map<number, MusicAsset> {
  const assets = new Map<number, MusicAsset>();
  for (const track of tracks) {
    if (track.verdict === "ok" && track.bytes) {
      const url = URL.createObjectURL(new Blob([track.bytes as BlobPart], { type: "audio/mpeg" }));
      assets.set(track.index, { url, path: track.path });
    }
  }
  return assets;
}

export async function buildBattleSoundAssets(audioContext: AudioContext, sounds: LoadedSound[], music: LoadedMusicVerdict[]): Promise<SoundAssets> {
  return { sounds: await decodeBattleSoundBuffers(audioContext, sounds), music: buildBattleMusicAssets(music) };
}
