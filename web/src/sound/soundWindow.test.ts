import { describe, expect, it } from "vitest";
import { readSoundWindow } from "./soundWindow";

// Байтовое смещение окна нарочно не ноль: сам `readSoundWindow` трактует нулевой указатель как
// «окно ещё не выделено» (см. отдельный тест ниже), а в настоящей памяти движка ноль и не бывает
// адресом валидной аллокации.
const WINDOW_BYTE_OFFSET = 16;

function makeWindow(cells: number[]): { memory: WebAssembly.Memory; ptr: number; len: number } {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const view = new Int32Array(memory.buffer, WINDOW_BYTE_OFFSET, cells.length);
  view.set(cells);
  return { memory, ptr: WINDOW_BYTE_OFFSET, len: cells.length };
}

describe("readSoundWindow", () => {
  it("читает включённый звук, номер трека и ни одной поднятой отметки", () => {
    const { memory, ptr, len } = makeWindow([1, 4, 0, 0, 0]);
    expect(readSoundWindow(memory, ptr, len)).toEqual({ enabled: true, musicId: 4, raisedSoundIds: [] });
  });

  it("читает выключенный звук и тишину (−1) как отсутствие трека", () => {
    const { memory, ptr, len } = makeWindow([0, -1, 0]);
    expect(readSoundWindow(memory, ptr, len)).toEqual({ enabled: false, musicId: null, raisedSoundIds: [] });
  });

  it("собирает поднятые отметки по их номеру, а не по позиции в окне", () => {
    const { memory, ptr, len } = makeWindow([1, -1, 0, 1, 1, 0, 1]);
    expect(readSoundWindow(memory, ptr, len).raisedSoundIds).toEqual([1, 2, 4]);
  });

  it("отдаёт пустой снимок для нулевого указателя — окно ещё не выделено (до load())", () => {
    const { memory, len } = makeWindow([1, 0, 0]);
    expect(readSoundWindow(memory, 0, len)).toEqual({ enabled: false, musicId: null, raisedSoundIds: [] });
  });
});
