import { describe, expect, it, vi } from "vitest";
import { applyParticles, invalidParticleKindNames, particleFieldErrorsByKind, particleKindsWithValue, particleTableOf, readParticleKinds } from "./particlesFile";

const FILE = `{
  "дым": { "image": "puff", "rate": 6, "lifetime": [4, 6], "size": 0.6 },
  "искры": { "image": "spark", "rate": 20, "lifetime": 1, "size": 0.2 }
}`;

describe("readParticleKinds", () => {
  it("виды идут в порядке файла, а не по алфавиту", () => {
    expect(readParticleKinds(FILE).map((kind) => kind.name)).toEqual(["дым", "искры"]);
  });

  it("имя из цифр не перескакивает вперёд, как ключ объекта JavaScript", () => {
    const kinds = readParticleKinds('{ "b": {}, "2": {}, "a": {}, "1": {} }');
    expect(kinds.map((kind) => kind.name)).toEqual(["b", "2", "a", "1"]);
  });

  it("поля вида — как в файле, без правки", () => {
    expect(readParticleKinds(FILE)[0]?.fields).toEqual({ image: "puff", rate: 6, lifetime: [4, 6], size: 0.6 });
  });

  it("вид, который не объект, остаётся карточкой без полей", () => {
    expect(readParticleKinds('{ "дым": 5 }')).toEqual([{ name: "дым", fields: {} }]);
  });

  it("нет файла, не JSON и не объект — видов нет", () => {
    expect(readParticleKinds(null)).toEqual([]);
    expect(readParticleKinds("{")).toEqual([]);
    expect(readParticleKinds("[1]")).toEqual([]);
  });
});

describe("particleKindsWithValue", () => {
  const kinds = readParticleKinds(FILE);

  it("заменяет поле только у названного вида", () => {
    const next = particleKindsWithValue(kinds, "дым", "rate", 9);
    expect(next[0]?.fields.rate).toBe(9);
    expect(next[1]).toBe(kinds[1]);
  });

  it("undefined убирает ключ, исходные виды не меняются", () => {
    const next = particleKindsWithValue(kinds, "дым", "rate", undefined);
    expect("rate" in (next[0]?.fields ?? {})).toBe(false);
    expect(kinds[0]?.fields.rate).toBe(6);
  });
});

describe("particleTableOf", () => {
  it("таблица «имя → вид» для движка", () => {
    expect(particleTableOf(readParticleKinds(FILE))).toEqual({
      дым: { image: "puff", rate: 6, lifetime: [4, 6], size: 0.6 },
      искры: { image: "spark", rate: 20, lifetime: 1, size: 0.2 },
    });
  });
});

describe("applyParticles", () => {
  it("зовёт set_wind_particles с таблицей видов и ничего не отдаёт, когда движок принял", () => {
    const setWindParticles = vi.fn(() => ({ ok: true }));
    const table = particleTableOf(readParticleKinds(FILE));

    expect(applyParticles({ set_wind_particles: setWindParticles }, table)).toBeUndefined();
    expect(setWindParticles).toHaveBeenCalledWith({ particles: table });
  });

  it("отдаёт текст ошибки движка", () => {
    expect(applyParticles({ set_wind_particles: () => ({ ok: false, error: "дым → rate: должно быть больше нуля" }) }, {})).toBe("дым → rate: должно быть больше нуля");
  });
});

describe("invalidParticleKindNames", () => {
  const errors = [
    { file: "particles.json", path: "дым → rate" },
    { file: "particles.json", path: "искры → lifetime → [0]" },
    { file: "scene.json", path: "objects → 0" },
    { file: "particles.json", path: "" },
  ];

  it("вид с ошибкой — первое звено пути ошибки в файле видов", () => {
    expect(invalidParticleKindNames(errors, "particles.json")).toEqual(new Set(["дым", "искры"]));
  });

  it("файла видов нет — помеченных нет", () => {
    expect(invalidParticleKindNames(errors, null)).toEqual(new Set());
  });
});

describe("particleFieldErrorsByKind", () => {
  const errors = [
    { file: "particles.json", path: "дым → rate", message: "rate: должно быть больше нуля" },
    { file: "particles.json", path: "дым → rate → [0]", message: "вторая ошибка того же поля" },
    { file: "particles.json", path: "искры → lifetime → [1]", message: "lifetime: пара не по возрастанию" },
    { file: "particles.json", path: "туман", message: "ошибка на весь вид" },
    { file: "scene.json", path: "дым → rate", message: "чужой файл" },
  ];

  it("вид — первое звено пути, поле — второе, у поля остаётся первая ошибка; ошибка на весь вид — под ключом \"\", чужого файла не берётся", () => {
    expect(particleFieldErrorsByKind(errors, "particles.json")).toEqual(
      new Map([
        ["дым", { rate: "rate: должно быть больше нуля" }],
        ["искры", { lifetime: "lifetime: пара не по возрастанию" }],
        ["туман", { "": "ошибка на весь вид" }],
      ]),
    );
  });

  it("файла видов нет — ошибок у полей нет", () => {
    expect(particleFieldErrorsByKind(errors, null).size).toBe(0);
  });
});
