import { describe, expect, it } from "vitest";
import { describeDeleteCause, describeRuleKind, withCodeErrorLine } from "./battleTypes";

describe("describeDeleteCause", () => {
  it("правило — его имя как есть", () => {
    expect(describeDeleteCause({ kind: "rule", rule: "rules[3]" })).toBe("rules[3]");
  });

  it("код игры — функция и вызвавшее её правило", () => {
    expect(describeDeleteCause({ kind: "code", function: "explode", rule: "rules[1]" })).toBe("explode (rules[1])");
  });

  it("истёкший срок жизни — требуемая фраза", () => {
    expect(describeDeleteCause({ kind: "lifetime" })).toBe("срок жизни истёк");
  });
});

describe("describeRuleKind", () => {
  it("подписывает каждый вид правила", () => {
    expect(describeRuleKind("move")).toBe("подвинуло");
    expect(describeRuleKind("collide")).toBe("столкнуло");
    expect(describeRuleKind("spawn")).toBe("создало");
  });
});

describe("withCodeErrorLine", () => {
  it("ошибки кода нет — список ошибок не меняется", () => {
    expect(withCodeErrorLine(null, ["движок: — сбой загрузки"])).toEqual(["движок: — сбой загрузки"]);
  });

  it("ошибка кода игры — первой строкой, тем же форматом, что на странице игры (требование 9)", () => {
    const codeError = { file: "код игры", path: "rules.lua", message: "attempt to call a nil value", line: 12, column: null };
    expect(withCodeErrorLine(codeError, [])).toEqual(["код игры: rules.lua (строка 12) — attempt to call a nil value"]);
  });

  it("ошибка кода вместе с ошибками проекта — перед ними", () => {
    const codeError = { file: "код игры", path: "rules.lua", message: "boom", line: null, column: null };
    expect(withCodeErrorLine(codeError, ["scene.json — не разобрать"])).toEqual(["код игры: rules.lua — boom", "scene.json — не разобрать"]);
  });
});
