// Случайность построителя — только от чисел описания: одно описание всегда даёт одни и те же файлы.

/** Генератор чисел от 0 до 1 (mulberry32): одно и то же `seed` — одна и та же последовательность. */
export function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Своё зерно для каждой операции из общего `seed` описания и `salt` — числа от имени операции или её номера. */
export function derivedSeed(seed, salt) {
  let h = (seed ^ 0x9e3779b9) >>> 0;
  h = Math.imul(h ^ (salt + 0x7f4a7c15), 0x85ebca6b) >>> 0;
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35) >>> 0;
  return (h ^ (h >>> 16)) >>> 0;
}
