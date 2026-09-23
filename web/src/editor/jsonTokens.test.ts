import { describe, expect, it } from "vitest";
import { splitJsonTokens } from "./jsonTokens";

function joinTokens(text: string): string {
  return splitJsonTokens(text)
    .map((token) => token.text)
    .join("");
}

describe("splitJsonTokens", () => {
  it("различает числа, строки и true/false/null", () => {
    expect(splitJsonTokens('[-1.5,"a",true,null]')).toEqual([
      { kind: "punctuation", text: "[" },
      { kind: "number", text: "-1.5" },
      { kind: "punctuation", text: "," },
      { kind: "string", text: '"a"' },
      { kind: "punctuation", text: "," },
      { kind: "literal", text: "true" },
      { kind: "punctuation", text: "," },
      { kind: "literal", text: "null" },
      { kind: "punctuation", text: "]" },
    ]);
  });

  it("строку перед двоеточием считает ключом", () => {
    expect(splitJsonTokens('{"interval":0.12}')).toEqual([
      { kind: "punctuation", text: "{" },
      { kind: "key", text: '"interval"' },
      { kind: "punctuation", text: ":" },
      { kind: "number", text: "0.12" },
      { kind: "punctuation", text: "}" },
    ]);
  });

  it("не путает цифры и слова внутри строки с числами и true", () => {
    expect(splitJsonTokens('"true 12 \\" -3"')).toEqual([{ kind: "string", text: '"true 12 \\" -3"' }]);
  });

  it("склеенные куски дают исходный текст", () => {
    const text = JSON.stringify({ ArrowUp: { press: [["velocity", [0, -1]], ["rotation", 270]] }, ok: false });
    expect(joinTokens(text)).toBe(text);
  });
});
