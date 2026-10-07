import { describe, expect, it } from "vitest";
import { PARTICLE_FIELD_GROUPS, type ParticleFieldSpec } from "./particleFieldSpecs";
import { formatParticleNumber, particleFieldText, parseParticleFieldText } from "./particleFieldValues";

function specOf(key: string): ParticleFieldSpec {
  const spec = PARTICLE_FIELD_GROUPS.flatMap((group) => group.fields).find((field) => field.key === key);
  if (spec === undefined) throw new Error(`нет поля ${key}`);
  return spec;
}

describe("группы полей", () => {
  it("три группы с подписями из плана", () => {
    expect(PARTICLE_FIELD_GROUPS.map((group) => [group.title, group.fields.map((field) => field.label)])).toEqual([
      ["Вылет", ["густота", "живёт, с", "скорость", "куда, °", "разброс, °"]],
      ["Вид", ["размер", "рост, раз", "видимость", "вращение, °/с"]],
      ["Полёт", ["тяжесть", "ветер, доля", "виляние"]],
    ]);
  });

  it("подсказка у каждого поля начинается с ключа файла", () => {
    for (const field of PARTICLE_FIELD_GROUPS.flatMap((group) => group.fields)) expect(field.hint.startsWith(`${field.key} — `)).toBe(true);
  });
});

describe("formatParticleNumber", () => {
  it("дробная часть через запятую, минус длинный — как в наброске вкладки", () => {
    expect(formatParticleNumber(0.6)).toBe("0,6");
    expect(formatParticleNumber(-20)).toBe("−20");
    expect(formatParticleNumber(4)).toBe("4");
  });
});

describe("particleFieldText", () => {
  it("ключа нет — значение по умолчанию, бледно; у обязательного ключа — пусто", () => {
    expect(particleFieldText(specOf("gravity"), undefined)).toEqual({ text: "0", isDefault: true });
    expect(particleFieldText(specOf("wind"), undefined)).toEqual({ text: "1", isDefault: true });
    expect(particleFieldText(specOf("rate"), undefined)).toEqual({ text: "", isDefault: true });
    expect(particleFieldText(specOf("lifetime"), undefined)).toEqual({ text: "", isDefault: true });
  });

  it("пара — одно поле «от – до», число — само", () => {
    expect(particleFieldText(specOf("lifetime"), [4, 6])).toEqual({ text: "4 – 6", isDefault: false });
    expect(particleFieldText(specOf("spin"), [-20, 20])).toEqual({ text: "−20 – 20", isDefault: false });
    expect(particleFieldText(specOf("size"), 0.5)).toEqual({ text: "0,5", isDefault: false });
  });

  it("видимость — точки через точку посередине", () => {
    expect(particleFieldText(specOf("opacity"), [0, 0.7, 0])).toEqual({ text: "0 · 0,7 · 0", isDefault: false });
    expect(particleFieldText(specOf("opacity"), 0.5)).toEqual({ text: "0,5", isDefault: false });
  });

  it("значение не того вида видно, как записано", () => {
    expect(particleFieldText(specOf("rate"), "много")).toEqual({ text: '"много"', isDefault: false });
  });
});

describe("parseParticleFieldText", () => {
  it("число с запятой или точкой; пустое поле убирает ключ", () => {
    expect(parseParticleFieldText(specOf("rate"), "7,5")).toEqual({ isValid: true, value: 7.5 });
    expect(parseParticleFieldText(specOf("rate"), "7.5")).toEqual({ isValid: true, value: 7.5 });
    expect(parseParticleFieldText(specOf("rate"), "  ")).toEqual({ isValid: true, value: undefined });
  });

  it("не число — не принимается", () => {
    expect(parseParticleFieldText(specOf("rate"), "семь")).toEqual({ isValid: false });
    expect(parseParticleFieldText(specOf("lifetime"), "1 – x")).toEqual({ isValid: false });
    expect(parseParticleFieldText(specOf("opacity"), "0 · x")).toEqual({ isValid: false });
  });

  it("пара: тире, дефис или пробел между «от» и «до»; равные и одно число пишутся числом", () => {
    expect(parseParticleFieldText(specOf("lifetime"), "4 – 6")).toEqual({ isValid: true, value: [4, 6] });
    expect(parseParticleFieldText(specOf("lifetime"), "4-6")).toEqual({ isValid: true, value: [4, 6] });
    expect(parseParticleFieldText(specOf("lifetime"), "4 6")).toEqual({ isValid: true, value: [4, 6] });
    expect(parseParticleFieldText(specOf("size"), "0,6 – 0,9")).toEqual({ isValid: true, value: [0.6, 0.9] });
    expect(parseParticleFieldText(specOf("lifetime"), "4 – 4")).toEqual({ isValid: true, value: 4 });
    expect(parseParticleFieldText(specOf("lifetime"), "5")).toEqual({ isValid: true, value: 5 });
  });

  it("пара с минусами: короткий и длинный", () => {
    expect(parseParticleFieldText(specOf("spin"), "−20 – 20")).toEqual({ isValid: true, value: [-20, 20] });
    expect(parseParticleFieldText(specOf("spin"), "-20-20")).toEqual({ isValid: true, value: [-20, 20] });
    expect(parseParticleFieldText(specOf("spin"), "-30 – -10")).toEqual({ isValid: true, value: [-30, -10] });
    expect(parseParticleFieldText(specOf("spin"), "−5")).toEqual({ isValid: true, value: -5 });
  });

  it("минус второго числа после пробела — его знак, а не разделитель", () => {
    expect(parseParticleFieldText(specOf("spin"), "-20 -10")).toEqual({ isValid: true, value: [-20, -10] });
    expect(parseParticleFieldText(specOf("spin"), "4 -6")).toEqual({ isValid: true, value: [4, -6] });
    expect(parseParticleFieldText(specOf("spin"), "-6--4")).toEqual({ isValid: true, value: [-6, -4] });
    expect(parseParticleFieldText(specOf("spin"), "4 - 6")).toEqual({ isValid: true, value: [4, 6] });
  });

  it("очень малое число показывается с порядком и так же разбирается", () => {
    const text = particleFieldText(specOf("size"), [1e-7, 2]).text;

    expect(parseParticleFieldText(specOf("size"), text)).toEqual({ isValid: true, value: [1e-7, 2] });
  });

  it("видимость: точки через точку посередине, пробел или запятую с пробелом; одна точка — число", () => {
    expect(parseParticleFieldText(specOf("opacity"), "0 · 0,7 · 0")).toEqual({ isValid: true, value: [0, 0.7, 0] });
    expect(parseParticleFieldText(specOf("opacity"), "0 0,7 0")).toEqual({ isValid: true, value: [0, 0.7, 0] });
    expect(parseParticleFieldText(specOf("opacity"), "0, 0.7, 0")).toEqual({ isValid: true, value: [0, 0.7, 0] });
    expect(parseParticleFieldText(specOf("opacity"), "0,5")).toEqual({ isValid: true, value: 0.5 });
  });
});
