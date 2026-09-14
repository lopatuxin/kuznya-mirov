import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Короткие звуки демо-игр синтезируются, а не скачиваются: так у них нет лицензии, которую надо
// помнить, и их можно пересобрать с другой высотой или длиной правкой одного числа.
// Запуск: `node scripts/generateDemoSounds.mjs` из `web/`. Результат детерминирован.

const SAMPLE_RATE = 44100;
const PEAK = 0.6;

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const gamesDir = resolve(scriptDir, "../../games");

function makeNoise(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x80000000 - 1;
  };
}

function square(phase) {
  return phase % 1 < 0.5 ? 1 : -1;
}

function triangle(phase) {
  const p = phase % 1;
  return p < 0.5 ? 4 * p - 1 : 3 - 4 * p;
}

// Тон с частотой, скользящей от `fromHz` к `toHz`, и экспоненциальным затуханием. Фаза копится
// по отсчётам, иначе скольжение частоты даёт щелчки.
function sweep({ seconds, fromHz, toHz, wave, decay }) {
  const count = Math.round(seconds * SAMPLE_RATE);
  const out = new Float64Array(count);
  let phase = 0;
  for (let i = 0; i < count; i++) {
    const t = i / count;
    phase += (fromHz + (toHz - fromHz) * t) / SAMPLE_RATE;
    out[i] = wave(phase) * Math.exp(-decay * t);
  }
  return out;
}

function concat(parts) {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Float64Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function mix(a, b, bGain) {
  const out = new Float64Array(Math.max(a.length, b.length));
  for (let i = 0; i < out.length; i++) {
    out[i] = (a[i] ?? 0) + (b[i] ?? 0) * bGain;
  }
  return out;
}

// Короткие нарастание и спад по краям убирают щелчок в начале и в конце файла.
function fadeEdges(samples) {
  const edge = Math.min(Math.round(0.004 * SAMPLE_RATE), Math.floor(samples.length / 2));
  for (let i = 0; i < edge; i++) {
    const gain = i / edge;
    samples[i] *= gain;
    samples[samples.length - 1 - i] *= gain;
  }
  return samples;
}

function normalize(samples) {
  let max = 0;
  for (const s of samples) max = Math.max(max, Math.abs(s));
  if (max === 0) return samples;
  return samples.map((s) => (s / max) * PEAK);
}

function encodeWav(samples) {
  const dataBytes = samples.length * 2;
  const buffer = Buffer.alloc(44 + dataBytes);
  buffer.write("RIFF", 0, "ascii");
  buffer.writeUInt32LE(36 + dataBytes, 4);
  buffer.write("WAVE", 8, "ascii");
  buffer.write("fmt ", 12, "ascii");
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22);
  buffer.writeUInt32LE(SAMPLE_RATE, 24);
  buffer.writeUInt32LE(SAMPLE_RATE * 2, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write("data", 36, "ascii");
  buffer.writeUInt32LE(dataBytes, 40);
  samples.forEach((s, i) => {
    const clamped = Math.max(-1, Math.min(1, s));
    buffer.writeInt16LE(Math.round(clamped * 32767), 44 + i * 2);
  });
  return buffer;
}

function noiseBurst(seconds, decay, seed) {
  const noise = makeNoise(seed);
  const count = Math.round(seconds * SAMPLE_RATE);
  const out = new Float64Array(count);
  for (let i = 0; i < count; i++) {
    out[i] = noise() * Math.exp(-decay * (i / count));
  }
  return out;
}

const SOUNDS = {
  "snake/sounds/eat.wav": () =>
    sweep({ seconds: 0.12, fromHz: 520, toHz: 1040, wave: square, decay: 3 }),
  "snake/sounds/hit.wav": () =>
    mix(
      sweep({ seconds: 0.4, fromHz: 180, toHz: 60, wave: triangle, decay: 5 }),
      noiseBurst(0.25, 8, 7),
      0.5,
    ),
  "arkanoid/sounds/bounce.wav": () =>
    sweep({ seconds: 0.06, fromHz: 660, toHz: 620, wave: triangle, decay: 4 }),
  "arkanoid/sounds/brick.wav": () =>
    sweep({ seconds: 0.1, fromHz: 900, toHz: 1400, wave: square, decay: 4 }),
  "arkanoid/sounds/win.wav": () =>
    concat(
      [523.25, 659.25, 783.99, 1046.5].map((hz, i) =>
        sweep({ seconds: i === 3 ? 0.4 : 0.13, fromHz: hz, toHz: hz, wave: square, decay: 2 }),
      ),
    ),
  "arkanoid/sounds/lose.wav": () =>
    sweep({ seconds: 0.9, fromHz: 440, toHz: 110, wave: square, decay: 2.5 }),
};

for (const [relativePath, synthesize] of Object.entries(SOUNDS)) {
  const target = resolve(gamesDir, relativePath);
  mkdirSync(dirname(target), { recursive: true });
  const wav = encodeWav(fadeEdges(normalize(synthesize())));
  writeFileSync(target, wav);
  console.log(`${relativePath}: ${wav.length} байт`);
}
