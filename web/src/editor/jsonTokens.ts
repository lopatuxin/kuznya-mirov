export type JsonTokenKind = "key" | "string" | "number" | "literal" | "punctuation";
export type JsonToken = { kind: JsonTokenKind; text: string };

const TOKEN_PATTERN = /("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d[\d.eE+-]*)|\b(true|false|null)\b/g;

function kindOf(match: RegExpMatchArray): JsonTokenKind {
  if (match[1] !== undefined) return match[2] !== undefined ? "key" : "string";
  if (match[3] !== undefined) return "number";
  return "literal";
}

/**
 * Режет компактный JSON значения свойства на куски для подсветки в панели свойств. Склеенные
 * обратно куски дают исходный текст символ в символ — значение показывается так, как записано.
 */
export function splitJsonTokens(jsonText: string): JsonToken[] {
  const tokens: JsonToken[] = [];
  let cursor = 0;
  for (const match of jsonText.matchAll(TOKEN_PATTERN)) {
    const start = match.index ?? 0;
    if (start > cursor) tokens.push({ kind: "punctuation", text: jsonText.slice(cursor, start) });
    const kind = kindOf(match);
    if (kind === "key") {
      const keyText = match[1] as string;
      tokens.push({ kind, text: keyText });
      tokens.push({ kind: "punctuation", text: match[0].slice(keyText.length) });
    } else {
      tokens.push({ kind, text: match[0] });
    }
    cursor = start + match[0].length;
  }
  if (cursor < jsonText.length) tokens.push({ kind: "punctuation", text: jsonText.slice(cursor) });
  return tokens;
}
