import { describe, expect, it } from "vitest";
import { formatErrors, formatErrorScreen } from "./engineErrors";

describe("formatErrors", () => {
  it("соединяет файл, путь и сообщение в одну строку на ошибку", () => {
    const text = formatErrors([
      { file: "scene.json", path: "objects[2].position", message: "ожидалось число", line: null, column: null },
    ]);
    expect(text).toBe("scene.json: objects[2].position — ожидалось число");
  });

  it("для ошибки без пути показывает только файл и сообщение", () => {
    const text = formatErrors([
      { file: "game.json", path: "", message: "битый JSON на позиции 12", line: null, column: null },
    ]);
    expect(text).toBe("game.json — битый JSON на позиции 12");
  });

  it("с известной позицией вставляет строку и столбец перед тире", () => {
    const text = formatErrors([
      {
        file: "scene.json",
        path: "objects[3] → velocity",
        message: "ожидалась пара чисел, получено строка",
        line: 4,
        column: 34,
      },
    ]);
    expect(text).toBe("scene.json: objects[3] → velocity (строка 4, столбец 34) — ожидалась пара чисел, получено строка");
  });

  it("выводит каждую ошибку отдельной строкой, ничего не теряя, независимо от того, есть ли позиция", () => {
    const text = formatErrors([
      { file: "game.json", path: "", message: "битый JSON на позиции 12", line: null, column: null },
      { file: "rules.json", path: "rules[0].kind", message: "неизвестный вид правила", line: 7, column: 3 },
    ]);
    expect(text.split("\n")).toHaveLength(2);
    expect(text).toBe(
      "game.json — битый JSON на позиции 12\nrules.json: rules[0].kind (строка 7, столбец 3) — неизвестный вид правила",
    );
  });
});

describe("formatErrorScreen", () => {
  it("ошибка плюс предупреждения: добавляет их отдельным разделом ниже ошибок", () => {
    const text = formatErrorScreen(
      [{ file: "rules.json", path: "rules[0].kind", message: "неизвестный вид правила", line: null, column: null }],
      [
        {
          file: "properties.json",
          path: "properties[3]",
          message: "свойство «color» никто не читает",
          line: null,
          column: null,
        },
      ],
    );
    expect(text).toBe(
      "rules.json: rules[0].kind — неизвестный вид правила\n\nПредупреждения:\nproperties.json: properties[3] — свойство «color» никто не читает",
    );
  });

  it("предупреждения с позицией форматируются так же, как ошибки", () => {
    const text = formatErrorScreen(
      [{ file: "rules.json", path: "rules[0].kind", message: "неизвестный вид правила", line: null, column: null }],
      [
        {
          file: "scene.json",
          path: "objects[3] → velocity",
          message: "ожидалась пара чисел, получено строка",
          line: 4,
          column: 34,
        },
      ],
    );
    expect(text).toBe(
      "rules.json: rules[0].kind — неизвестный вид правила\n\nПредупреждения:\nscene.json: objects[3] → velocity (строка 4, столбец 34) — ожидалась пара чисел, получено строка",
    );
  });

  it("без предупреждений раздел не добавляется вовсе", () => {
    const errors = [{ file: "game.json", path: "", message: "битый JSON на позиции 12", line: null, column: null }];
    expect(formatErrorScreen(errors, [])).toBe(formatErrors(errors));
  });

  it("отказ на первом шаге (read_entry) показывает и его ошибки, и его предупреждения", () => {
    const errors = [{ file: "game.json", path: "extra_field", message: "неизвестный ключ", line: 3, column: 2 }];
    const warnings = [{ file: "game.json", path: "name", message: "имя игры пустое", line: 1, column: 9 }];
    const text = formatErrorScreen(errors, warnings);
    expect(text).toContain("game.json: extra_field (строка 3, столбец 2) — неизвестный ключ");
    expect(text).toContain("Предупреждения:\ngame.json: name (строка 1, столбец 9) — имя игры пустое");
  });
});
