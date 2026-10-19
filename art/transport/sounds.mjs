import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Звуки Транспортной компании синтезируются, а не скачиваются: чужой лицензии в них нет, и любой
// можно пересобрать правкой числа. `cash` — касса, когда клиент заплатил: стук ящика, звонок и звон
// монет. `notify` — мягкий двузвучный сигнал остальных уведомлений, тише кассы.
// Запуск: `node art/transport/sounds.mjs` из корня репозитория. Результат детерминирован.

const RATE = 44100;
const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../games/transport/sounds");

function makeNoise(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x80000000 - 1;
  };
}

// Затухающий синус с короткой атакой: из таких частичных тонов собраны звонок и сигнал.
function addTone(out, { at, hz, amp, tau, attack = 0.002 }) {
  const start = Math.round(at * RATE);
  for (let i = start; i < out.length; i++) {
    const t = (i - start) / RATE;
    const env = Math.min(1, t / attack) * Math.exp(-t / tau);
    if (t > attack && env < 1e-5) break;
    out[i] += amp * env * Math.sin(2 * Math.PI * hz * t);
  }
}

// Затухающий шум через полосовой фильтр с добротностью `q`: низкая — глухой стук, высокая — звон.
function addClick(out, { at, hz, q, amp, tau, seed }) {
  const noise = makeNoise(seed);
  const w = (2 * Math.PI * hz) / RATE;
  const alpha = Math.sin(w) / (2 * q);
  const a0 = 1 + alpha;
  const a1 = -2 * Math.cos(w);
  const a2 = 1 - alpha;
  let x1 = 0;
  let x2 = 0;
  let y1 = 0;
  let y2 = 0;
  const start = Math.round(at * RATE);
  const count = Math.min(out.length - start, Math.round(tau * 10 * RATE));
  for (let k = 0; k < count; k++) {
    const x = noise() * Math.exp(-k / RATE / tau);
    const y = (alpha * x - alpha * x2 - a1 * y1 - a2 * y2) / a0;
    x2 = x1;
    x1 = x;
    y2 = y1;
    y1 = y;
    out[start + k] += amp * y;
  }
}

// Громкость по пику и короткое затухание в конце, чтобы файл не обрывался щелчком.
function finish(out, peak, fadeSeconds) {
  const top = out.reduce((m, v) => Math.max(m, Math.abs(v)), 0);
  const fade = Math.round(fadeSeconds * RATE);
  for (let i = 0; i < out.length; i++) {
    const tail = Math.min(1, (out.length - 1 - i) / fade);
    out[i] = (out[i] / top) * peak * tail;
  }
  return out;
}

function cash() {
  const out = new Float64Array(Math.round(1.6 * RATE));
  // Ящик кассы: два металлических щелчка и глухой удар.
  addClick(out, { at: 0, hz: 1800, q: 3, amp: 0.9, tau: 0.004, seed: 11 });
  addTone(out, { at: 0, hz: 150, amp: 0.35, tau: 0.03 });
  addClick(out, { at: 0.035, hz: 2600, q: 4, amp: 0.6, tau: 0.003, seed: 12 });
  // Звонок: неровные частичные тона колокольчика, два близких основных дают дрожание.
  const bell = 2093;
  for (const [ratio, amp, tau] of [[1, 0.5, 0.5], [1.0045, 0.35, 0.5], [2.756, 0.22, 0.3], [5.404, 0.1, 0.15]]) {
    addTone(out, { at: 0.07, hz: bell * ratio, amp, tau });
  }
  // Монеты: короткие звонкие щелчки вразброс.
  const random = makeNoise(7);
  for (let i = 0; i < 7; i++) {
    const at = 0.09 + i * 0.045 + (random() + 1) * 0.012;
    const hz = 5200 + (random() + 1) * 1500;
    addClick(out, { at, hz, q: 25, amp: 0.25 - i * 0.02, tau: 0.02, seed: 100 + i });
  }
  return finish(out, 0.8, 0.15);
}

function notify() {
  const out = new Float64Array(Math.round(0.6 * RATE));
  addTone(out, { at: 0, hz: 880, amp: 0.5, tau: 0.16, attack: 0.004 });
  addTone(out, { at: 0, hz: 3520, amp: 0.08, tau: 0.03, attack: 0.004 });
  addTone(out, { at: 0.11, hz: 1318.5, amp: 0.45, tau: 0.2, attack: 0.004 });
  addTone(out, { at: 0.11, hz: 5274, amp: 0.06, tau: 0.03, attack: 0.004 });
  return finish(out, 0.45, 0.05);
}

// WAV: 16 бит, один канал.
function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => data.writeInt16LE(Math.round(Math.max(-1, Math.min(1, v)) * 32767), i * 2));
  const head = Buffer.alloc(44);
  head.write("RIFF", 0);
  head.writeUInt32LE(36 + data.length, 4);
  head.write("WAVE", 8);
  head.write("fmt ", 12);
  head.writeUInt32LE(16, 16);
  head.writeUInt16LE(1, 20);
  head.writeUInt16LE(1, 22);
  head.writeUInt32LE(RATE, 24);
  head.writeUInt32LE(RATE * 2, 28);
  head.writeUInt16LE(2, 32);
  head.writeUInt16LE(16, 34);
  head.write("data", 36);
  head.writeUInt32LE(data.length, 40);
  return Buffer.concat([head, data]);
}

mkdirSync(outDir, { recursive: true });
writeFileSync(resolve(outDir, "cash.wav"), wav(cash()));
writeFileSync(resolve(outDir, "notify.wav"), wav(notify()));
