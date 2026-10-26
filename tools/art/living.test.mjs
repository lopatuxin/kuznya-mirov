import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { pingpongFrame } from "./living.mjs";

describe("pingpongFrame", () => {
  it("ролик идёт вперёд, потом назад без повторов крайних кадров и по кругу сходится с началом", () => {
    const frames = Array.from({ length: 2 * 4 - 2 }, (_, k) => pingpongFrame(k, 4));
    assert.deepEqual(frames, [0, 1, 2, 3, 2, 1]);
  });
});
