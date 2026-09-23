import { describe, expect, it } from "vitest";
import { formatRussianCount } from "./russianPlural";

const ERROR_FORMS: [string, string, string] = ["ошибка", "ошибки", "ошибок"];

describe("formatRussianCount", () => {
  it("ставит слово в форму по последним цифрам числа", () => {
    expect(formatRussianCount(1, ERROR_FORMS)).toBe("1 ошибка");
    expect(formatRussianCount(3, ERROR_FORMS)).toBe("3 ошибки");
    expect(formatRussianCount(5, ERROR_FORMS)).toBe("5 ошибок");
    expect(formatRussianCount(21, ERROR_FORMS)).toBe("21 ошибка");
    expect(formatRussianCount(104, ERROR_FORMS)).toBe("104 ошибки");
  });

  it("11–14 всегда во множественной форме", () => {
    expect(formatRussianCount(11, ERROR_FORMS)).toBe("11 ошибок");
    expect(formatRussianCount(12, ERROR_FORMS)).toBe("12 ошибок");
    expect(formatRussianCount(114, ERROR_FORMS)).toBe("114 ошибок");
  });
});
