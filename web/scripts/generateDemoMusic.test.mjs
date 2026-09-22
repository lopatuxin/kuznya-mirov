import { describe, expect, it } from "vitest";
import { MUSIC, SAMPLE_RATE, buildKorobeinikiTrack, encodeMp3 } from "./generateDemoMusic.mjs";

const ID3V2_MAGIC = [0x49, 0x44, 0x33]; // "ID3"

function startsWithId3v2OrFrameSync(bytes) {
  const isId3v2 = bytes[0] === ID3V2_MAGIC[0] && bytes[1] === ID3V2_MAGIC[1] && bytes[2] === ID3V2_MAGIC[2];
  // MPEG frame sync: 11 старших бит установлены (0xFF, затем 0xE0..0xFF).
  const isFrameSync = bytes[0] === 0xff && (bytes[1] & 0xe0) === 0xe0;
  return isId3v2 || isFrameSync;
}

describe("buildKorobeinikiTrack", () => {
  const samples = buildKorobeinikiTrack();

  it("даёт непустой трек", () => {
    expect(samples.length).toBeGreaterThan(0);
  });

  it("укладывается в разумную длину трека — от 30 до 90 секунд", () => {
    const seconds = samples.length / SAMPLE_RATE;
    expect(seconds).toBeGreaterThanOrEqual(30);
    expect(seconds).toBeLessThanOrEqual(90);
  });

  it("не клиппингует — пик по модулю строго меньше единицы, с запасом", () => {
    let max = 0;
    for (const s of samples) max = Math.max(max, Math.abs(s));
    expect(max).toBeGreaterThan(0);
    expect(max).toBeLessThanOrEqual(0.9);
  });
});

describe("MUSIC — договор с данными игры", () => {
  it("объявляет ровно музыку партии тетриса", () => {
    expect(Object.keys(MUSIC)).toEqual(["tetris/music/game.mp3"]);
  });
});

describe("encodeMp3", () => {
  it("начинается с ID3v2-заголовка или синхрослова MPEG-фрейма", () => {
    const mp3 = encodeMp3(buildKorobeinikiTrack());
    expect(mp3.length).toBeGreaterThan(0);
    expect(startsWithId3v2OrFrameSync(mp3)).toBe(true);
  });

  it("даёт разумный размер файла для 128 кбит/с моно", () => {
    const samples = buildKorobeinikiTrack();
    const seconds = samples.length / SAMPLE_RATE;
    const mp3 = encodeMp3(samples);
    const expectedBytes = seconds * (128_000 / 8);
    // Кадрирование MP3 (1152 отсчёта на фрейм) и хвост кодировщика дают отклонение в пределах фрейма.
    expect(mp3.length).toBeGreaterThan(expectedBytes * 0.9);
    expect(mp3.length).toBeLessThan(expectedBytes * 1.1);
  });
});
