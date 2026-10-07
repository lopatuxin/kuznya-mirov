import { describe, expect, it } from "vitest";
import {
  copiedParticleKindName,
  particlesTextWithKind,
  particlesTextWithoutKind,
  particlesTextWithRenamedKind,
  particlesTextWithValue,
  sceneTextWithoutParticles,
  sceneTextWithRenamedParticles,
} from "./particlesTextEditing";

// Ручное оформление: вид на строке, выравнивание у первого — оно должно остаться байт в байт.
const FILE = `{
  "дым":   { "image": "puff", "rate": 6, "lifetime": [4, 6], "size": 0.6 },
  "искры": { "image": "spark", "rate": 20, "lifetime": 1, "size": 0.2 },
  "лист": { "image": "leaf", "rate": 2, "lifetime": 5, "size": 0.4 }
}
`;

describe("particlesTextWithValue", () => {
  it("меняется только значение, остальной текст байт в байт", () => {
    expect(particlesTextWithValue(FILE, "дым", "rate", 9)).toBe(FILE.replace('"rate": 6', '"rate": 9'));
  });

  it("пара пишется в стиле файла, с пробелом после запятой", () => {
    expect(particlesTextWithValue(FILE, "дым", "lifetime", [3, 5])).toBe(FILE.replace("[4, 6]", "[3, 5]"));
  });

  it("новый ключ дописывается последним в свой вид, в одну строку с остальными", () => {
    expect(particlesTextWithValue(FILE, "искры", "gravity", 1.5)).toBe(FILE.replace('"size": 0.2 }', '"size": 0.2, "gravity": 1.5 }'));
  });

  it("новый ключ в виде, набранном по строке на ключ, встаёт на своей строке с отступом соседа", () => {
    const multiline = '{\r\n  "дым": {\r\n    "image": "puff",\r\n    "rate": 6\r\n  }\r\n}\r\n';
    expect(particlesTextWithValue(multiline, "дым", "grow", 3)).toBe('{\r\n  "дым": {\r\n    "image": "puff",\r\n    "rate": 6,\r\n    "grow": 3\r\n  }\r\n}\r\n');
  });

  it("undefined убирает ключ вместе с запятой", () => {
    expect(particlesTextWithValue(FILE, "дым", "size", undefined)).toBe(FILE.replace(', "size": 0.6', ""));
    expect(particlesTextWithValue(FILE, "дым", "image", undefined)).toBe(FILE.replace('"image": "puff", ', ""));
  });

  it("убрать ключ, которого нет, или править вид, которого нет, — текст не меняется", () => {
    expect(particlesTextWithValue(FILE, "дым", "grow", undefined)).toBe(FILE);
    expect(particlesTextWithValue(FILE, "нет", "rate", 1)).toBe(FILE);
  });
});

describe("particlesTextWithKind", () => {
  it("новый вид последним в файле, в стиле файла: на своей строке с отступом соседа", () => {
    const text = particlesTextWithKind(FILE, "огонь", { image: "fire", rate: 5 });
    expect(text).toBe(FILE.replace('"size": 0.4 }\n}', '"size": 0.4 },\n  "огонь": { "image": "fire", "rate": 5 }\n}'));
  });

  it("в пустую таблицу вид встаёт на свою строку, перевод строки — как у файла", () => {
    expect(particlesTextWithKind("{}", "дым", { image: "puff", rate: 5 })).toBe('{\n  "дым": { "image": "puff", "rate": 5 }\n}');
    expect(particlesTextWithKind("{}\r\n", "дым", { rate: 5 })).toBe('{\r\n  "дым": { "rate": 5 }\r\n}\r\n');
  });

  it("следующий вид после первого ложится под него", () => {
    const first = particlesTextWithKind("{}", "дым", { rate: 5 });
    expect(particlesTextWithKind(first, "искры", { rate: 9 })).toBe('{\n  "дым": { "rate": 5 },\n  "искры": { "rate": 9 }\n}');
  });
});

describe("particlesTextWithoutKind", () => {
  it("средний вид пропадает вместе со строкой, соседи не тронуты", () => {
    expect(particlesTextWithoutKind(FILE, "искры")).toBe(FILE.replace(/ {2}"искры".*\n/, ""));
  });

  it("первый вид пропадает, второй встаёт на его место", () => {
    expect(particlesTextWithoutKind(FILE, "дым")).toBe(FILE.replace(/ {2}"дым".*\n/, ""));
  });

  it("последний вид пропадает вместе с запятой предыдущего", () => {
    expect(particlesTextWithoutKind(FILE, "лист")).toBe(FILE.replace(/,\n {2}"лист".*\n/, "\n"));
  });

  it("единственный вид — остаётся пустая таблица", () => {
    expect(particlesTextWithoutKind('{ "дым": { "rate": 1 } }', "дым")).toBe("{}");
  });
});

describe("particlesTextWithRenamedKind", () => {
  it("меняется только ключ вида, остальной текст байт в байт", () => {
    expect(particlesTextWithRenamedKind(FILE, "искры", "звёзды")).toBe(FILE.replace('"искры"', '"звёзды"'));
  });

  it("имя со служебными знаками пишется экранированным", () => {
    expect(particlesTextWithRenamedKind('{ "а": {} }', "а", 'б"в')).toBe('{ "б\\"в": {} }');
  });

  it("вида нет — текст не меняется", () => {
    expect(particlesTextWithRenamedKind(FILE, "нет", "х")).toBe(FILE);
  });
});

describe("имена видов", () => {
  it("копия — «<имя>-копия», занятое — «<имя>-копия-2»", () => {
    expect(copiedParticleKindName("дым", ["дым"])).toBe("дым-копия");
    expect(copiedParticleKindName("дым", ["дым", "дым-копия"])).toBe("дым-копия-2");
  });
});

describe("источники сцены при переименовании и удалении вида", () => {
  const SCENE = `{
  "objects": [
    { "position": [1, 1], "size": [1, 1], "particles": "дым" },
    { "position": [2, 1], "size": [1, 1], "particles": "искры" },
    { "position": [3, 1], "size": [1, 1], "image": "wall" },
    { "position": [4, 1], "size": [1, 1], "particles": "дым", "layer": 2 }
  ]
}`;

  it("переименование меняет particles у всех источников с прежним именем", () => {
    expect(sceneTextWithRenamedParticles(SCENE, "дым", "туман")).toBe(SCENE.split('"particles": "дым"').join('"particles": "туман"'));
  });

  it("удаление снимает particles только у источников этого вида", () => {
    const text = sceneTextWithoutParticles(SCENE, "дым");
    const objects = (JSON.parse(text) as { objects: Record<string, unknown>[] }).objects;
    expect(objects.map((object) => object.particles)).toEqual([undefined, "искры", undefined, undefined]);
    expect(objects[3]?.layer).toBe(2);
  });

  it("источников нет — сцена не меняется", () => {
    expect(sceneTextWithRenamedParticles(SCENE, "нет", "х")).toBe(SCENE);
    expect(sceneTextWithoutParticles(SCENE, "нет")).toBe(SCENE);
  });
});
