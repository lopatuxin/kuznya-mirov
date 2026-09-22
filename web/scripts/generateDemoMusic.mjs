import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// Внутренние файлы `lamejs` ссылаются друг на друга голыми именами (`MPEGMode`, `Lame`, …) без
// `require`, поэтому `import { Mp3Encoder } from "lamejs"` падает с `MPEGMode is not defined`.
// Пакет сам поставляет `lame.all.js` — все файлы в одной функции `lamejs`; её и берём, ничего не
// выкладывая в `globalThis`.
function loadLamejs() {
  const require = createRequire(import.meta.url);
  const bundle = readFileSync(require.resolve("lamejs/lame.all.js"), "utf8");
  return new Function(`${bundle}
return lamejs;`)();
}

const { Mp3Encoder } = loadLamejs();

// «Коробейники», русская народная песня XIX века (слова — Н. Некрасов) на мелодию, которая
// является общественным достоянием. Ниже — только эта мелодия (прямоугольная волна, скважность
// 25% в части A и 50% в части B — оттенок тембра между частями, как это делают настоящие NES-игры).
// Бас (треугольная волна) и лёгкий шумовой ритм — свои, не аранжировка Nintendo (Хирокадзу Танака):
// корни аккордов только Am/E/Am/Dm/Am/E/Am, без её конкретных партий баса и гармонии.
// Форма — часть A дважды, часть B дважды; этот цикл (16 тактов, ~25.6 с при 150 уд/мин) повторён
// в файле дважды подряд, чтобы уложиться в разумную длину файла (see generateDemoMusic.test.mjs).
// Такты стыкуются без паузы, но кодировщик MP3 добавляет короткую тишину в начале и в конце файла,
// так что на стыке петли в браузере слышен короткий провал («Звук»: бесшовной петли MP3 нет).
// Запуск: `node scripts/generateDemoMusic.mjs` из `web/`. Результат детерминирован.

export const SAMPLE_RATE = 44100;
const TEMPO_BPM = 150;
const BEAT_SECONDS = 60 / TEMPO_BPM;
const PEAK = 0.5;

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const gamesDir = resolve(scriptDir, "../../games");

const NOTES = {
  D2: 73.4162,
  E2: 82.4069,
  A2: 110.0,
  B2: 123.47,
  E3: 164.81,
  A4: 440.0,
  B4: 493.88,
  C5: 523.25,
  D5: 587.33,
  E5: 659.25,
  F5: 698.46,
  G5: 783.99,
  A5: 880.0,
};

function makeNoise(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x80000000 - 1;
  };
}

function square(phase, duty) {
  return phase % 1 < duty ? 1 : -1;
}

function triangle(phase) {
  const p = phase % 1;
  return p < 0.5 ? 4 * p - 1 : 3 - 4 * p;
}

// Короткая атака и затухание у каждой ноты: без них одинаковые ноты подряд сливаются в одну, а на
// стыке нот щёлкает скачок волны.
const ATTACK_SECONDS = 0.004;
const RELEASE_SECONDS = 0.03;

function tone(freqHz, beats, wave) {
  const count = Math.round(beats * BEAT_SECONDS * SAMPLE_RATE);
  const out = new Float64Array(count);
  if (freqHz === null) return out;
  const step = freqHz / SAMPLE_RATE;
  const attack = Math.max(1, Math.round(ATTACK_SECONDS * SAMPLE_RATE));
  const release = Math.max(1, Math.round(RELEASE_SECONDS * SAMPLE_RATE));
  let phase = 0;
  for (let i = 0; i < count; i++) {
    const envelope = Math.min(1, i / attack, (count - i) / release);
    out[i] = wave(phase) * envelope;
    phase += step;
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

function mix(parts) {
  const length = Math.max(...parts.map(([samples]) => samples.length));
  const out = new Float64Array(length);
  for (const [samples, gain] of parts) {
    for (let i = 0; i < samples.length; i++) out[i] += samples[i] * gain;
  }
  return out;
}

function normalize(samples, peak) {
  let max = 0;
  for (const s of samples) max = Math.max(max, Math.abs(s));
  if (max === 0) return samples;
  const scale = peak / max;
  return samples.map((s) => s * scale);
}

// Мелодия «Коробейников», как её обычно расписывают: часть A (такты 1–4, ниже) и часть B
// (такты 5–8, ответная фраза, поднимается до A5). `null` — пауза: восьмая перед частью B и
// последняя доля тактов 4 и 8.
const PART_A = [
  ["E5", 1], ["B4", 0.5], ["C5", 0.5], ["D5", 1], ["C5", 0.5], ["B4", 0.5],
  ["A4", 1], ["A4", 0.5], ["C5", 0.5], ["E5", 1], ["D5", 0.5], ["C5", 0.5],
  ["B4", 1.5], ["C5", 0.5], ["D5", 1], ["E5", 1],
  ["C5", 1], ["A4", 1], ["A4", 1], [null, 1],
];

const PART_B = [
  [null, 0.5], ["D5", 1], ["F5", 0.5], ["A5", 1], ["G5", 0.5], ["F5", 0.5],
  ["E5", 1.5], ["C5", 0.5], ["E5", 1], ["D5", 0.5], ["C5", 0.5],
  ["B4", 1], ["B4", 0.5], ["C5", 0.5], ["D5", 1], ["E5", 1],
  ["C5", 1], ["A4", 1], ["A4", 1], [null, 1],
];

const LEAD_DUTY_A = 0.25;
const LEAD_DUTY_B = 0.5;

// Свои корни аккордов, по одному на такт: Am, E, Am, Dm — часть A; Am, E, Am, Am — часть B
// (семь смен из «Am/E/Am/Dm/Am/E/Am», последний такт каждой части просто держит предыдущий).
const CHORDS_A = [
  ["A2", "E3"], ["E2", "B2"], ["A2", "E3"], ["D2", "A2"],
];
const CHORDS_B = [
  ["A2", "E3"], ["E2", "B2"], ["A2", "E3"], ["A2", "E3"],
];

function withDuty(notes, duty) {
  return notes.map(([note, beats]) => ({ note, beats, duty }));
}

function renderMelody(events) {
  return concat(
    events.map(({ note, beats, duty }) =>
      tone(note === null ? null : NOTES[note], beats, (phase) => square(phase, duty)),
    ),
  );
}

// Простой «бум-бум-квинта-бум» на четвертях — свой бас, не партия Танаки.
function renderBass(bars) {
  const parts = [];
  for (const [rootKey, fifthKey] of bars) {
    const root = NOTES[rootKey];
    const fifth = NOTES[fifthKey];
    parts.push(tone(root, 1, triangle), tone(root, 1, triangle), tone(fifth, 1, triangle), tone(root, 1, triangle));
  }
  return concat(parts);
}

// Тихий щелчок на каждую слабую долю (восьмую «и») — лёгкий шумовой ритм, а не полноценная ударная партия.
function renderNoiseRhythm(totalBeats) {
  const noise = makeNoise(20260921);
  const tickCount = Math.round(0.03 * SAMPLE_RATE);
  const beatSamples = Math.round(BEAT_SECONDS * SAMPLE_RATE);
  const halfBeatSamples = Math.round((BEAT_SECONDS / 2) * SAMPLE_RATE);
  const totalSamples = Math.round(totalBeats * BEAT_SECONDS * SAMPLE_RATE);
  const out = new Float64Array(totalSamples);
  for (let pos = halfBeatSamples; pos < totalSamples; pos += beatSamples) {
    for (let i = 0; i < tickCount && pos + i < totalSamples; i++) {
      out[pos + i] += noise() * Math.exp(-40 * (i / tickCount));
    }
  }
  return out;
}

export function buildKorobeinikiTrack() {
  const cycleEvents = [
    ...withDuty(PART_A, LEAD_DUTY_A),
    ...withDuty(PART_A, LEAD_DUTY_A),
    ...withDuty(PART_B, LEAD_DUTY_B),
    ...withDuty(PART_B, LEAD_DUTY_B),
  ];
  const cycleBars = [...CHORDS_A, ...CHORDS_A, ...CHORDS_B, ...CHORDS_B];
  // Цикл AABB повторён дважды подряд — стык не пауза, а тот же такт, что и в начале.
  const events = [...cycleEvents, ...cycleEvents];
  const bars = [...cycleBars, ...cycleBars];

  const melody = renderMelody(events);
  const bass = renderBass(bars);
  const totalBeats = events.reduce((sum, event) => sum + event.beats, 0);
  const noise = renderNoiseRhythm(totalBeats);

  return normalize(
    mix([
      [melody, 0.9],
      [bass, 0.55],
      [noise, 0.25],
    ]),
    PEAK,
  );
}

export function encodeMp3(samples) {
  const encoder = new Mp3Encoder(1, SAMPLE_RATE, 128);
  const FRAME_SAMPLES = 1152;
  const int16 = new Int16Array(samples.length);
  for (let i = 0; i < samples.length; i++) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    int16[i] = Math.round(clamped * 32767);
  }

  const chunks = [];
  for (let i = 0; i < int16.length; i += FRAME_SAMPLES) {
    const encoded = encoder.encodeBuffer(int16.subarray(i, i + FRAME_SAMPLES));
    if (encoded.length > 0) chunks.push(Buffer.from(encoded));
  }
  const tail = encoder.flush();
  if (tail.length > 0) chunks.push(Buffer.from(tail));
  return Buffer.concat(chunks);
}

// Экспортируется, чтобы тест сверял длительность и уровень громкости, не запуская запись на диск.
export const MUSIC = {
  "tetris/music/game.mp3": buildKorobeinikiTrack,
};

// Модуль импортируется тестом напрямую (см. generateDemoMusic.test.mjs) — без этой проверки такой
// импорт тут же перезаписывал бы настоящую музыку игры.
const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  for (const [relativePath, synthesize] of Object.entries(MUSIC)) {
    const target = resolve(gamesDir, relativePath);
    mkdirSync(dirname(target), { recursive: true });
    const mp3 = encodeMp3(synthesize());
    writeFileSync(target, mp3);
    console.log(`${relativePath}: ${mp3.length} байт`);
  }
}
