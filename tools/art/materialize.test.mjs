import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { duration, letterMasks, letterPhase, over } from "./materialize.mjs";

describe("letterMasks", () => {
  it("маска буквы — точки, которые она добавила к строке, в долях 0..1", () => {
    const empty = Uint8Array.from([0, 0, 0, 0]);
    const first = Uint8Array.from([255, 0, 0, 0]);
    const both = Uint8Array.from([255, 0, 51, 255]);
    const [a, b] = letterMasks([empty, first, both]);
    assert.deepEqual([...a], [1, 0, 0, 0]);
    assert.deepEqual([...b], [0, 0, 0.2, 1].map(Math.fround));
  });
});

describe("letterPhase", () => {
  it("до начала буквы ничего нет", () => {
    assert.deepEqual(letterPhase(-0.1), { grow: 0, dissolve: 0, reveal: 0, cool: 0, glow: 0 });
  });

  it("к концу буквы дымка рассеялась, буква открыта, остыла и светится", () => {
    assert.deepEqual(letterPhase(5), { grow: 1, dissolve: 1, reveal: 1, cool: 1, glow: 1 });
  });

  it("полоса света поднимается, пока дымка ещё стоит", () => {
    const middle = letterPhase(0.3);
    assert.ok(middle.reveal > 0 && middle.reveal < 1, `reveal ${middle.reveal}`);
    assert.ok(middle.dissolve < 0.5, `dissolve ${middle.dissolve}`);
  });
});

describe("duration", () => {
  it("в последнем кадре последняя буква остыла, а её дымка рассеялась", () => {
    const delay = 0.5;
    const letters = 11;
    const lastFrame = (Math.round(duration(letters, delay) * 24) - 1) / 24;
    const phase = letterPhase(lastFrame - delay - (letters - 1) * 0.08);
    assert.equal(phase.dissolve, 1);
    assert.equal(phase.glow, 1);
  });
});

describe("over", () => {
  it("слой с полным покрытием закрывает кадр, с половинным — смешивается", () => {
    const frame = new Float32Array([0, 0, 0, 0]);
    over(frame, 0, [200, 100, 0], 0.5);
    assert.deepEqual([...frame], [100, 50, 0, 0.5]);
    over(frame, 0, [10, 20, 30], 1);
    assert.deepEqual([...frame], [10, 20, 30, 1]);
  });
});
