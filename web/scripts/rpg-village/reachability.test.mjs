import { describe, expect, it } from "vitest";
import { isReachable, reachableCells } from "./reachability.mjs";

describe("reachableCells / isReachable", () => {
  it("без препятствий достижима вся сетка", () => {
    const visited = reachableCells(3, 2, [], 0, 0);
    for (let y = 0; y < 2; y++) for (let x = 0; x < 3; x++) expect(isReachable(visited, x, y)).toBe(true);
  });

  it("сплошная стена из прямоугольников делит сетку на две недостижимые друг от друга части", () => {
    const wall = [{ position: [1, 0], size: [1, 3] }];
    const visited = reachableCells(3, 3, wall, 0, 0);
    expect(isReachable(visited, 0, 1)).toBe(true); // своя сторона
    expect(isReachable(visited, 2, 1)).toBe(false); // за стеной
  });

  it("разрыв в стене шириной в клетку делает вторую половину достижимой", () => {
    const wallWithGap = [
      { position: [1, 0], size: [1, 1] },
      // разрыв в строке 1
      { position: [1, 2], size: [1, 1] },
    ];
    const visited = reachableCells(3, 3, wallWithGap, 0, 1);
    expect(isReachable(visited, 2, 1)).toBe(true);
  });

  it("клетка вне сетки не считается достижимой", () => {
    const visited = reachableCells(2, 2, [], 0, 0);
    expect(isReachable(visited, 5, 5)).toBe(false);
    expect(isReachable(visited, -1, 0)).toBe(false);
  });
});
