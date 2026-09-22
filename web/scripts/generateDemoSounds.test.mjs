import { describe, expect, it } from "vitest";
import { SAMPLE_RATE, SOUNDS, encodeWav, sweep } from "./generateDemoSounds.mjs";

describe("sweep", () => {
  it("возвращает `seconds * SAMPLE_RATE` отсчётов", () => {
    const samples = sweep({ seconds: 0.1, fromHz: 440, toHz: 440, wave: (phase) => Math.sin(phase), decay: 0 });
    expect(samples.length).toBe(Math.round(0.1 * SAMPLE_RATE));
  });
});

describe("encodeWav", () => {
  it("пишет заголовок RIFF/WAVE и частоту дискретизации", () => {
    const wav = encodeWav(new Float64Array([0, 0.5, -0.5]));
    expect(wav.subarray(0, 4).toString("ascii")).toBe("RIFF");
    expect(wav.subarray(8, 12).toString("ascii")).toBe("WAVE");
    expect(wav.readUInt32LE(24)).toBe(SAMPLE_RATE);
    expect(wav.length).toBe(44 + 3 * 2);
  });
});

// «Файлы, которые делают скрипты» — звуки тетриса не длиннее 5 секунд.
describe("SOUNDS — договор с данными игр", () => {
  const tetrisSounds = Object.entries(SOUNDS).filter(([path]) => path.startsWith("tetris/"));

  it("список звуков тетриса совпадает с договором", () => {
    expect(tetrisSounds.map(([path]) => path).sort()).toEqual(
      [
        "tetris/sounds/move.wav",
        "tetris/sounds/rotate.wav",
        "tetris/sounds/land.wav",
        "tetris/sounds/clear.wav",
        "tetris/sounds/tetris.wav",
        "tetris/sounds/level.wav",
        "tetris/sounds/over.wav",
      ].sort(),
    );
  });

  it.each(tetrisSounds)("%s не длиннее 5 секунд", (_path, synthesize) => {
    const seconds = synthesize().length / SAMPLE_RATE;
    expect(seconds).toBeLessThanOrEqual(5);
  });
});
