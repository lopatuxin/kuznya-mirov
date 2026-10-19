import { describe, expect, it } from "vitest";
import { createModelObject, readModelSizes } from "./projectModels";

describe("readModelSizes", () => {
  it("модели по порядку, который отдал движок, с размером в клетках", () => {
    expect(
      readModelSizes([
        { name: "house", size: [8, 6, 7] },
        { name: "birch", size: [2.5, 2.5, 9] },
      ]),
    ).toEqual([
      { name: "house", size: [8, 6, 7] },
      { name: "birch", size: [2.5, 2.5, 9] },
    ]);
  });

  it("до загрузки игры и при чужом ответе — пустой список, запись не того вида пропускается", () => {
    expect(readModelSizes(undefined)).toEqual([]);
    expect(readModelSizes([])).toEqual([]);
    expect(readModelSizes([null, { name: 5, size: [1, 1, 1] }, { name: "a", size: [1, 1] }, { name: "b", size: [1, "x", 1] }, { name: "ok", size: [1, 2, 3] }])).toEqual([{ name: "ok", size: [1, 2, 3] }]);
  });
});

describe("createModelObject", () => {
  it("size — место модели без height, как в файле, середина — под указателем, position — левый верхний угол", () => {
    expect(createModelObject({ name: "house", size: [8, 6, 7] }, [10, 20])).toEqual({ position: [6, 17], size: [8, 6], model: "house" });
  });

  it("шум машинного счёта в размере убирается, position — до сотой клетки", () => {
    const object = createModelObject({ name: "birch", size: [2.5000000001, 2.4999999999, 9] }, [3.337, 5]);

    expect(object).toEqual({ position: [2.09, 3.75], size: [2.5, 2.5], model: "birch" });
  });
});
