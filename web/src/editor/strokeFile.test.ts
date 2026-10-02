import { describe, expect, it } from "vitest";
import { parseStrokeFile } from "./strokeFile";

const MATERIALS = ["grass", "rock"];
const RAISE = { brush: "raise", size: 6, strength: 50, seconds: 2, points: [[40, 12], [46, 14]] };
const PAINT = { brush: "paint", material: "rock", size: 3, strength: 80, seconds: 1, points: [[52, 20]] };

function messageOf(strokes: unknown[]): string | null {
  const result = parseStrokeFile(JSON.stringify(strokes), MATERIALS);
  return result.status === "error" ? result.message : null;
}

describe("parseStrokeFile — верный файл", () => {
  it("разбирает мазки кистей рельефа и «Покрасить»", () => {
    const result = parseStrokeFile(JSON.stringify([RAISE, { ...PAINT, shift: true }]), MATERIALS);

    expect(result).toEqual({
      status: "ok",
      strokes: [
        { brush: "raise", size: 6, strength: 50, seconds: 2, points: [[40, 12], [46, 14]], isShift: false, material: null },
        { brush: "paint", size: 3, strength: 80, seconds: 1, points: [[52, 20]], isShift: true, material: "rock" },
      ],
    });
  });

  it("пустой список — мазков нет", () => {
    expect(parseStrokeFile("[]", MATERIALS)).toEqual({ status: "ok", strokes: [] });
  });

  it("границы: size 1 и 64, strength 1 и 100, дробные секунды", () => {
    expect(messageOf([{ ...RAISE, size: 1, strength: 1, seconds: 0.01 }, { ...RAISE, size: 64, strength: 100 }])).toBeNull();
  });

  it("level и smooth разбираются", () => {
    expect(messageOf([{ ...RAISE, brush: "level" }, { ...RAISE, brush: "smooth" }])).toBeNull();
  });
});

describe("parseStrokeFile — ошибки называют мазок с единицы и поле", () => {
  it("не JSON", () => {
    const result = parseStrokeFile("[{", MATERIALS);

    expect(result.status === "error" && result.message.startsWith("файл мазков не разбирается как JSON")).toBe(true);
  });

  it("не список", () => {
    expect(parseStrokeFile('{"brush":"raise"}', MATERIALS)).toEqual({ status: "error", message: "файл мазков: ожидается список мазков" });
  });

  it("мазок не объект", () => {
    expect(messageOf([RAISE, 5])).toBe("мазок 2: ожидается объект");
    expect(messageOf([[]])).toBe("мазок 1: ожидается объект");
    expect(messageOf([null])).toBe("мазок 1: ожидается объект");
  });

  it("неизвестный ключ", () => {
    expect(messageOf([RAISE, { ...RAISE, speed: 3 }])).toContain("мазок 2 → speed: неизвестный ключ");
  });

  it("нет обязательного поля — каждого из пяти", () => {
    for (const field of ["brush", "size", "strength", "seconds", "points"]) {
      const stroke: Record<string, unknown> = { ...RAISE };
      delete stroke[field];

      expect(messageOf([stroke])).toBe(`мазок 1 → ${field}: обязательного поля нет`);
    }
  });

  it("неизвестная кисть, и erode тоже — до Фазы 22", () => {
    expect(messageOf([{ ...RAISE, brush: "carve" }])).toContain("мазок 1 → brush: нужна одна из кистей");
    expect(messageOf([{ ...RAISE, brush: "erode" }])).toContain("erode придёт в Фазе 22");
    expect(messageOf([{ ...RAISE, brush: 7 }])).toContain("мазок 1 → brush:");
  });

  it("size — не число от 1 до 64", () => {
    for (const size of [0, 0.5, 65, "6", null, Number.NaN]) {
      expect(messageOf([RAISE, { ...RAISE, size }])).toBe("мазок 2 → size: нужно число от 1 до 64");
    }
  });

  it("strength — не число от 1 до 100", () => {
    for (const strength of [0, 101, "50", null]) {
      expect(messageOf([{ ...RAISE, strength }])).toBe("мазок 1 → strength: нужно число от 1 до 100");
    }
  });

  it("seconds — не число больше нуля", () => {
    for (const seconds of [0, -1, "2", null]) {
      expect(messageOf([{ ...RAISE, seconds }])).toBe("мазок 1 → seconds: нужно число больше нуля");
    }
  });

  it("points — не непустой список пар чисел", () => {
    for (const points of [[], "x", [[1]], [[1, 2, 3]], [[1, "2"]], [1, 2], null, [[1, 2], []]]) {
      expect(messageOf([{ ...RAISE, points }])).toBe("мазок 1 → points: нужен непустой список пар чисел [x, y]");
    }
  });

  it("shift — не true и не false", () => {
    for (const shift of ["true", 1, null]) {
      expect(messageOf([{ ...RAISE, shift }])).toBe("мазок 1 → shift: нужно true или false");
    }
  });

  it("у paint нет material, он не строка или не объявлен в files.materials", () => {
    const withoutMaterial: Record<string, unknown> = { ...PAINT };
    delete withoutMaterial.material;

    expect(messageOf([withoutMaterial])).toBe("мазок 1 → material: у paint нужна строка с материалом из files.materials");
    expect(messageOf([{ ...PAINT, material: 5 }])).toBe("мазок 1 → material: у paint нужна строка с материалом из files.materials");
    expect(messageOf([PAINT, { ...PAINT, material: "lava" }])).toBe("мазок 2 → material: материала «lava» нет в files.materials");
  });

  it("material у кисти рельефа", () => {
    expect(messageOf([{ ...RAISE, material: "rock" }])).toBe("мазок 1 → material: материал есть только у paint");
  });
});
