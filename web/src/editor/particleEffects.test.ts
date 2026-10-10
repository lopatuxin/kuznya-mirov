import { describe, expect, it } from "vitest";
import {
  colorForPicker,
  degreesAtPoint,
  describeSparksFan,
  effectsOfObject,
  formatParticleNumber,
  normalizeDegrees,
  PARTICLE_COLOR_NAMES,
  PARTICLE_DEFAULTS,
  PARTICLE_PROPERTY_NAMES,
  particleEffectById,
  presentEffectKeys,
  sparksDirectionAtPoint,
  sparksSpreadAtPoint,
  translateParticleKeys,
} from "./particleEffects";

describe("свойства и эффекты", () => {
  it("двенадцать встроенных свойств частиц и огня, у каждого эффекта своё главное", () => {
    expect(PARTICLE_PROPERTY_NAMES).toEqual(["smoke", "smoke_height", "smoke_color", "sparks", "sparks_reach", "sparks_direction", "sparks_spread", "leaf_fall", "leaf_color", "fire", "fire_color", "fire_glow"]);
    expect(["smoke", "sparks", "leaves", "fire"].map((id) => particleEffectById(id)?.mainKey)).toEqual(["smoke", "sparks", "leaf_fall", "fire"]);
    expect(particleEffectById("snow")).toBeUndefined();
  });

  it("эффекты объекта идут по порядку «Дым», «Искры», «Листья», «Огонь», а группа с нулём в главном свойстве остаётся", () => {
    expect(effectsOfObject({ fire: 0.5, leaf_fall: 0.3, smoke: 0 }).map((effect) => effect.id)).toEqual(["smoke", "leaves", "fire"]);
    expect(effectsOfObject({ fire: 0 }).map((effect) => effect.id)).toEqual(["fire"]);
    expect(effectsOfObject({ position: [1, 1] })).toEqual([]);
    expect(effectsOfObject(null)).toEqual([]);
  });

  it("настройка без главного свойства группу не открывает", () => {
    expect(effectsOfObject({ smoke_height: 6, sparks_reach: 3, fire_color: "#ff8c1a", fire_glow: 0.5 })).toEqual([]);
  });

  it("«Убрать» берёт только те ключи эффекта, что есть у объекта", () => {
    const sparks = particleEffectById("sparks");
    expect(sparks && presentEffectKeys(sparks, { position: [1, 1], sparks: 0.5, sparks_spread: 10, smoke: 0.5 })).toEqual(["sparks", "sparks_spread"]);
  });
});

describe("огонь", () => {
  it("карточка «огонь»: подписи, значение при отпускании, «Убрать» берёт три свойства", () => {
    const fire = particleEffectById("fire");

    expect(fire).toMatchObject({ cardLabel: "огонь", title: "Огонь", mainKey: "fire", dropValue: 0.5, densityLabels: ["тлеет", "бушует"] });
    expect(fire && presentEffectKeys(fire, { fire: 1, fire_color: "#2255ff", fire_glow: 0.2, smoke: 0.5 })).toEqual(["fire", "fire_color", "fire_glow"]);
  });

  it("fire_color правится палитрой, как остальные цвета эффектов; умолчания — оранжевый и 0,5", () => {
    expect(PARTICLE_COLOR_NAMES).toContain("fire_color");
    expect(PARTICLE_DEFAULTS).toMatchObject({ fire_color: "#ff8c1a", fire_glow: 0.5 });
  });

  it("в тексте ошибки fire, fire_color и fire_glow названы русскими подписями, чужие слова не задеваются", () => {
    expect(translateParticleKeys("fire: нужно от 0 до 1 включительно, получено 2")).toBe("сила огня: нужно от 0 до 1 включительно, получено 2");
    expect(translateParticleKeys("fire_glow: нужно от 0 до 1 включительно, получено 2; fire_color: цвет должен быть вида #rrggbb")).toBe(
      "яркость ореола: нужно от 0 до 1 включительно, получено 2; цвет пламени: цвет должен быть вида #rrggbb",
    );
    expect(translateParticleKeys("fireplace")).toBe("fireplace");
  });
});

describe("числа и цвет", () => {
  it("число пишется по-русски, с запятой", () => {
    expect(formatParticleNumber(1.5)).toBe("1,5");
    expect(formatParticleNumber(4)).toBe("4");
  });

  it("палитре браузера годится только #rrggbb, иначе запасной цвет", () => {
    expect(colorForPicker("#AA3300", "#a6a6ac")).toBe("#AA3300");
    expect(colorForPicker("red", "#a6a6ac")).toBe("#a6a6ac");
    expect(colorForPicker(undefined, "#a6a6ac")).toBe("#a6a6ac");
  });
});

describe("круг направления", () => {
  it("градусы по кругу: 450 — то же, что 90, минус — с другой стороны", () => {
    expect(normalizeDegrees(450)).toBe(90);
    expect(normalizeDegrees(-90)).toBe(270);
    expect(normalizeDegrees(360)).toBe(0);
  });

  it("угол точки: 0 — вверх, 90 — вправо, 180 — вниз, 270 — влево", () => {
    expect(degreesAtPoint(0, -10)).toBeCloseTo(0);
    expect(degreesAtPoint(10, 0)).toBeCloseTo(90);
    expect(degreesAtPoint(0, 10)).toBeCloseTo(180);
    expect(degreesAtPoint(-10, 0)).toBeCloseTo(270);
  });

  it("направление — целые градусы, с Ctrl — шагом 15°", () => {
    expect(sparksDirectionAtPoint(Math.sin((100 * Math.PI) / 180), -Math.cos((100 * Math.PI) / 180), false)).toBe(100);
    expect(sparksDirectionAtPoint(Math.sin((100 * Math.PI) / 180), -Math.cos((100 * Math.PI) / 180), true)).toBe(105);
    expect(sparksDirectionAtPoint(-0.01, -10, true)).toBe(0);
  });

  it("разброс — отклонение точки от направления в любую сторону, не больше 180", () => {
    expect(sparksSpreadAtPoint(10, 0, 0, false)).toBe(90);
    expect(sparksSpreadAtPoint(-10, 0, 0, false)).toBe(90);
    expect(sparksSpreadAtPoint(0, 10, 0, false)).toBe(180);
    expect(sparksSpreadAtPoint(0, -10, 350, false)).toBe(10);
    expect(sparksSpreadAtPoint(10, 0, 90, false)).toBe(0);
  });

  it("под кругом слова: «вверх, ±30°»; между сторонами — градусы", () => {
    expect(describeSparksFan(0, 30)).toBe("вверх, ±30°");
    expect(describeSparksFan(90, 45)).toBe("вправо, ±45°");
    expect(describeSparksFan(225, 10)).toBe("вниз-влево, ±10°");
    expect(describeSparksFan(450, 30)).toBe("вправо, ±30°");
    expect(describeSparksFan(20, 30)).toBe("20°, ±30°");
  });
});
