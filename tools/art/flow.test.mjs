import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { inside, layerWeights, waterness } from "./flow.mjs";

describe("layerWeights", () => {
  it("в начале и в конце круга веса одни и те же — видео идёт по кругу без рывка", () => {
    assert.deepEqual(layerWeights(0), layerWeights(1));
    assert.deepEqual(layerWeights(0.25), layerWeights(1.25));
  });

  it("слой виден целиком посреди своего хода и исчезает, пока сдвинут дальше всего", () => {
    assert.deepEqual(layerWeights(0.5), [1, 0]);
    assert.deepEqual(layerWeights(0), [0, 1]);
  });

  it("веса двух слоёв в сумме дают единицу", () => {
    for (const phase of [0.1, 0.3, 0.7, 0.9]) {
      const [a, b] = layerWeights(phase);
      assert.ok(Math.abs(a + b - 1) < 1e-9, `${phase}`);
    }
  });
});

describe("waterness", () => {
  it("синяя вода и белая пена — вода, камень, хвоя и кора — нет", () => {
    assert.equal(waterness(110, 180, 245), 1);
    assert.equal(waterness(235, 240, 250), 1);
    assert.equal(waterness(120, 105, 95), 0);
    assert.equal(waterness(30, 70, 40), 0);
    assert.equal(waterness(60, 70, 95), 0);
  });
});

describe("inside", () => {
  it("точка внутри многоугольника и снаружи", () => {
    const square = [[0, 0], [10, 0], [10, 10], [0, 10]];
    assert.equal(inside(square, 5, 5), true);
    assert.equal(inside(square, 15, 5), false);
  });
});
