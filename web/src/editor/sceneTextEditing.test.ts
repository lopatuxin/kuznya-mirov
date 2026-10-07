import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  addObjectProperty,
  addTerrainFilePath,
  appendSceneObject,
  declarePropertyKind,
  formatSceneValue,
  removeObjectProperty,
  removeSceneObject,
  sceneTextWithWind,
  setObjectPropertyValue,
  setObjectPropertyValues,
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

describe("setObjectPropertyValues", () => {
  it("меняет несколько свойств одним текстом, остальное байт в байт", () => {
    const result = setObjectPropertyValues(SCENE_TEXT, 1, { position: [2.5, 6.25], size: [2, 1.5] });
    expect(result).toBe(SCENE_TEXT.replace('"position": [0, 6], "size": [1, 1]', '"position": [2.5, 6.25], "size": [2, 1.5]'));
  });

  it("свойство, которого не было, дописывает в конец объекта", () => {
    const result = setObjectPropertyValues(SCENE_TEXT, 2, { rotation: 280, height: 1.5 });
    expect(result).toContain('"image": "wall", "layer": 1, "rotation": 280, "height": 1.5 }');
    expect(result.startsWith(SCENE_TEXT.slice(0, SCENE_TEXT.indexOf('"position": [11, 6]')))).toBe(true);
  });

  it("пустой набор оставляет текст как был", () => {
    expect(setObjectPropertyValues(SCENE_TEXT, 0, {})).toBe(SCENE_TEXT);
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

describe("addTerrainFilePath", () => {
  const RPG_GAME_TEXT = readFileSync(new URL("../../../games/rpg/game.json", import.meta.url), "utf8").replace(/\r\n/g, "\n");
  const RPG_WITHOUT_TERRAIN = RPG_GAME_TEXT.replace('    "terrain": "terrain.json",\n', "");

  it("дописывает ключ в конец files на своей строке с отступом соседа, остальной текст не меняется", () => {
    const result = addTerrainFilePath(RPG_WITHOUT_TERRAIN, "terrain.json");
    expect(JSON.parse(result).files.terrain).toBe("terrain.json");
    expect(result.replace(',\n    "terrain": "terrain.json"', "")).toBe(RPG_WITHOUT_TERRAIN);
    expect(result).toContain('    },\n    "terrain": "terrain.json"\n  }');
  });

  it("files в одну строку — ключ через запятую с пробелом", () => {
    expect(addTerrainFilePath('{ "files": { "scene": "scene.json" } }', "terrain-2.json")).toBe(
      '{ "files": { "scene": "scene.json", "terrain": "terrain-2.json" } }',
    );
  });

  it("перенос строки Windows сохраняется", () => {
    const result = addTerrainFilePath('{\r\n  "files": {\r\n    "scene": "scene.json"\r\n  }\r\n}', "terrain.json");
    expect(result).toBe('{\r\n  "files": {\r\n    "scene": "scene.json",\r\n    "terrain": "terrain.json"\r\n  }\r\n}');
  });

  it("путь в подпапке — как у files.scene", () => {
    const result = addTerrainFilePath('{ "files": { "scene": "world/scene.json" } }', "world/terrain.json");
    expect(JSON.parse(result).files.terrain).toBe("world/terrain.json");
  });
});

describe("sceneTextWithWind", () => {
  it("заменяет значение wind на месте, остальной текст байт в байт", () => {
    const text = '{\n  "wind": [1.5,   0],\n  "objects": [\n    { "position": [0, 6],  "size": [1, 1] }\n  ]\n}';
    expect(sceneTextWithWind(text, [-2, 0.5])).toBe(text.replace("[1.5,   0]", "[-2, 0.5]"));
  });

  it("[0, 0] пишется как есть", () => {
    expect(sceneTextWithWind('{ "wind": [3, 1], "objects": [] }', [0, 0])).toBe('{ "wind": [0, 0], "objects": [] }');
  });

  it("ключа не было — дописывает последним ключом корня на своей строке, ручное оформление остаётся", () => {
    const result = sceneTextWithWind(SCENE_TEXT, [1.5, 0]);
    expect(result).toBe(SCENE_TEXT.replace("\n  ]\n}", '\n  ],\n  "wind": [1.5, 0]\n}'));
    expect(result).toContain('"name": "wall_left_hidden",  "position"');
    expect(Object.keys(JSON.parse(result)).at(-1)).toBe("wind");
  });

  it("корень в одну строку — ключ через запятую с пробелом", () => {
    expect(sceneTextWithWind('{ "objects": [] }', [-2, 0])).toBe('{ "objects": [], "wind": [-2, 0] }');
  });

  it("перенос строки Windows сохраняется", () => {
    expect(sceneTextWithWind('{\r\n  "objects": []\r\n}', [1, 0])).toBe('{\r\n  "objects": [],\r\n  "wind": [1, 0]\r\n}');
  });

  it("пустой корень заполняет jsonc-parser", () => {
    expect(JSON.parse(sceneTextWithWind("{}", [1, 2]))).toEqual({ wind: [1, 2] });
  });
});
