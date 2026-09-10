import { describe, expect, it } from "vitest";
import { formatErrors } from "./engineErrors";
import { buildWarningsBadge, shouldBlurAfterToggleActivation } from "./warningsPanel";

describe("buildWarningsBadge", () => {
  it("не показывает плашку, когда предупреждений нет — успех без предупреждений", () => {
    expect(buildWarningsBadge([])).toBe(null);
  });

  it("успех с предупреждениями: считает их и форматирует список тем же способом, что и ошибки", () => {
    const warnings = [
      {
        file: "properties.json",
        path: "properties[3]",
        message: "свойство «color» никто не читает",
        line: null,
        column: null,
      },
      { file: "scene.json", path: "objects[5]", message: "объект стоит за пределами сцены", line: 12, column: 5 },
    ];
    const badge = buildWarningsBadge(warnings);
    expect(badge?.label).toBe("⚠ 2 предупреждения");
    expect(badge?.details).toBe(formatErrors(warnings));
  });

  it("предупреждение с позицией показывает строку и столбец в списке так же, как ошибка", () => {
    const warnings = [
      { file: "scene.json", path: "objects[3] → velocity", message: "ожидалась пара чисел, получено строка", line: 4, column: 34 },
    ];
    const badge = buildWarningsBadge(warnings);
    expect(badge?.details).toBe(
      "scene.json: objects[3] → velocity (строка 4, столбец 34) — ожидалась пара чисел, получено строка",
    );
  });

  it("согласует числительное с русским словом «предупреждение»", () => {
    expect(buildWarningsBadge([{ file: "a", path: "", message: "m", line: null, column: null }])?.label).toBe(
      "⚠ 1 предупреждение",
    );
    expect(
      buildWarningsBadge(Array(5).fill({ file: "a", path: "", message: "m", line: null, column: null }))?.label,
    ).toBe("⚠ 5 предупреждений");
    expect(
      buildWarningsBadge(Array(11).fill({ file: "a", path: "", message: "m", line: null, column: null }))?.label,
    ).toBe("⚠ 11 предупреждений");
    expect(
      buildWarningsBadge(Array(21).fill({ file: "a", path: "", message: "m", line: null, column: null }))?.label,
    ).toBe("⚠ 21 предупреждение");
  });
});

describe("shouldBlurAfterToggleActivation", () => {
  it("клик мышью (detail больше нуля) — фокус нужно снять", () => {
    expect(shouldBlurAfterToggleActivation({ detail: 1 })).toBe(true);
  });

  it("двойной и тройной клик мышью тоже снимают фокус", () => {
    expect(shouldBlurAfterToggleActivation({ detail: 2 })).toBe(true);
    expect(shouldBlurAfterToggleActivation({ detail: 3 })).toBe(true);
  });

  it("активация с клавиатуры (Enter/Space, detail 0) — фокус остаётся", () => {
    expect(shouldBlurAfterToggleActivation({ detail: 0 })).toBe(false);
  });
});
