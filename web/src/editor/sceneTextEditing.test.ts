import { describe, expect, it } from "vitest";
import {
  addObjectProperty,
  appendSceneObject,
  declarePropertyKind,
  formatSceneValue,
  removeObjectProperty,
  removeSceneObject,
  setObjectPropertyValue,
} from "./sceneTextEditing";

// Кусок `games/tetris/scene.json` с ручным выравниванием — «Редактор», критерии готовности «Страница».
const SCENE_TEXT = `{
  "objects": [
    { "name": "wall_left_hidden",  "position": [0, 0], "size": [1, 6], "solid": true, "color": "#0e1018", "layer": 1 },
    { "position": [0, 6], "size": [1, 1], "solid": true, "image": "wall", "layer": 1 },
    { "position": [11, 6], "size": [1, 1], "solid": true, "image": "wall", "layer": 1 }
  ]
}`;

describe("formatSceneValue", () => {
  it("пишет объект и вложенные массивы в одну строку с пробелами — требование 5", () => {
    expect(formatSceneValue({ position: [1, 6], size: [1, 1], image: "wall" })).toBe(
      '{ "position": [1, 6], "size": [1, 1], "image": "wall" }',
    );
  });

  it("пишет число как есть, строку в кавычках", () => {
    expect(formatSceneValue(3)).toBe("3");
    expect(formatSceneValue("wall")).toBe('"wall"');
    expect(formatSceneValue(true)).toBe("true");
  });

  it("пустой объект — без пробела внутри", () => {
    expect(formatSceneValue({})).toBe("{}");
  });
});

describe("setObjectPropertyValue", () => {
  it("заменяет только значение свойства, остальной текст байт в байт", () => {
    const result = setObjectPropertyValue(SCENE_TEXT, 1, "position", [3.57, 5.33]);
    expect(result).toBe(SCENE_TEXT.replace('"position": [0, 6]', '"position": [3.57, 5.33]'));
  });

  it("значение массивом пишется с пробелом после запятой", () => {
    const result = setObjectPropertyValue(SCENE_TEXT, 2, "position", [4, 5]);
    expect(result).toContain('"position": [4, 5]');
  });
});

describe("addObjectProperty / removeObjectProperty", () => {
  it("дописывает свойство в конец объекта — требование 5, вид «hp»: 3", () => {
    const result = addObjectProperty(SCENE_TEXT, 1, "hp", 3);
    expect(result).toBe(SCENE_TEXT.replace('"layer": 1 },\n    { "position": [11, 6]', '"layer": 1, "hp": 3 },\n    { "position": [11, 6]'));
    expect(result).toContain('"layer": 1, "hp": 3 }');
  });

  it("удаляет свойство объекта, остальной текст не двигается", () => {
    const result = removeObjectProperty(SCENE_TEXT, 1, "solid");
    expect(result).toBe(
      SCENE_TEXT.replace('{ "position": [0, 6], "size": [1, 1], "solid": true, "image"', '{ "position": [0, 6], "size": [1, 1], "image"'),
    );
  });

  it("удаление несуществующего свойства не меняет текст", () => {
    expect(removeObjectProperty(SCENE_TEXT, 1, "nope")).toBe(SCENE_TEXT);
  });
});

describe("appendSceneObject / removeSceneObject", () => {
  it("копия объекта дописывается в конец objects в одну строку с пробелами", () => {
    const objectCount = 3;
    const result = appendSceneObject(SCENE_TEXT, objectCount, { position: [1, 6], size: [1, 1], image: "wall" });
    expect(result).toBe(
      SCENE_TEXT.replace(
        '{ "position": [11, 6], "size": [1, 1], "solid": true, "image": "wall", "layer": 1 }\n  ]',
        '{ "position": [11, 6], "size": [1, 1], "solid": true, "image": "wall", "layer": 1 }, { "position": [1, 6], "size": [1, 1], "image": "wall" }\n  ]',
      ),
    );
  });

  it("удаление элемента из середины сдвигает номера, остальной текст байт в байт", () => {
    const result = removeSceneObject(SCENE_TEXT, 1);
    const parsed = JSON.parse(result) as { objects: unknown[] };
    expect(parsed.objects).toHaveLength(2);
    expect(result).not.toContain('"image": "wall", "layer": 1 },\n    { "position": [11, 6]');
    expect(result).toContain('"position": [11, 6]');
  });
});

describe("declarePropertyKind", () => {
  it("объявляет новое свойство автора в конце properties", () => {
    const propertiesText = `{
  "properties": {
    "level": "number",
    "score": "number"
  }
}`;
    const result = declarePropertyKind(propertiesText, "hp", "number");
    expect(result).toBe(propertiesText.replace('"score": "number"', '"score": "number", "hp": "number"'));
  });

  it("объявление в пустом properties — без ведущей запятой", () => {
    const propertiesText = `{ "properties": {} }`;
    const result = declarePropertyKind(propertiesText, "hp", "flag");
    expect(result).toBe('{ "properties": {"hp": "flag"} }');
  });
});
