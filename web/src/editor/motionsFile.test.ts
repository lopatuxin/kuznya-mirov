import { describe, expect, it } from "vitest";
import {
  EMPTY_MOTIONS_TEXT,
  effectiveBoneTurn,
  formatMotionsText,
  markIndexAt,
  motionsTextWithBoneTurn,
  motionsTextWithMark,
  motionsTextWithMovedMark,
  motionsTextWithNewMotion,
  motionsTextWithSetting,
  motionsTextWithoutBones,
  motionsTextWithoutMark,
  readBoneCorrections,
  readMotionNames,
  readMotionSettings,
  type TurnMark,
} from "./motionsFile";

const RUN = '{ "run": { "model": "hero", "clip": "Jog" } }';

function parsed(text: string | null): Record<string, Record<string, unknown>> {
  return JSON.parse(text ?? "null") as Record<string, Record<string, unknown>>;
}

function marks(...entries: [number, number][]): TurnMark[] {
  return entries.map(([at, z]) => ({ at, turn: [0, 0, z] }));
}

describe("чтение motions.json", () => {
  it("имена — в порядке файла, а файл, что не читается или не таблица, пуст", () => {
    expect(readMotionNames('{ "b": { "model": "m", "clip": "c" }, "a": { "model": "m", "clip": "c" } }')).toEqual(["b", "a"]);
    expect(readMotionNames("{")).toEqual([]);
    expect(readMotionNames("[1]")).toEqual([]);
    expect(readMotionNames(null)).toEqual([]);
  });

  it("ручки движения без ключей берут умолчания «Движений»", () => {
    expect(readMotionSettings(RUN, "run")).toEqual({ model: "hero", clip: "Jog", speed: 1, loop: null, upper: null, bounce: 1, lean: 0, armsOut: 0 });
    expect(readMotionSettings(RUN, "walk")).toBeNull();
  });

  it("поправки костей: тройка и отметки по возрастанию доли, всё прочее пропускается", () => {
    const text = JSON.stringify({
      run: {
        model: "hero",
        clip: "Jog",
        bones: { A: [1, 2, 3], B: [{ at: 0.5, turn: [0, 0, 5] }, { at: 0.1, turn: [0, 0, 1] }], C: "плохо", D: [1, 2] },
      },
    });

    expect(readBoneCorrections(text, "run")).toEqual({ A: [1, 2, 3], B: marks([0.1, 1], [0.5, 5]) });
  });
});

describe("новое движение", () => {
  it("первое движение заводит таблицу с моделью и движением файла", () => {
    const text = motionsTextWithNewMotion(null, "hero_jog", "hero", "Jog");

    expect(parsed(text)).toEqual({ hero_jog: { model: "hero", clip: "Jog" } });
    expect(text?.endsWith("\n")).toBe(true);
  });

  it("следующее встаёт в конец, а имя пустое или занятое — правки нет", () => {
    const text = motionsTextWithNewMotion(RUN, "sprint", "hero", "Sprint");

    expect(Object.keys(parsed(text))).toEqual(["run", "sprint"]);
    expect(motionsTextWithNewMotion(RUN, "run", "hero", "Jog")).toBeNull();
    expect(motionsTextWithNewMotion(RUN, "", "hero", "Jog")).toBeNull();
  });

  it("пустая таблица, которую оставляет отмена первого движения, читается как файл без движений", () => {
    expect(readMotionNames(EMPTY_MOTIONS_TEXT)).toEqual([]);
    expect(parsed(motionsTextWithNewMotion(EMPTY_MOTIONS_TEXT, "a", "m", "c"))).toEqual({ a: { model: "m", clip: "c" } });
  });
});

describe("общие ручки", () => {
  it("число пишется в описание, остальные ключи — в том числе reference, которого стенд не знает, — остаются", () => {
    const text = '{ "run": { "model": "hero", "clip": "Jog", "reference": { "video": "ref", "start": 1, "end": 2 } } }';

    const next = motionsTextWithSetting(text, "run", "lean", 12);

    expect(parsed(next).run).toEqual({ model: "hero", clip: "Jog", reference: { video: "ref", start: 1, end: 2 }, lean: 12 });
  });

  it("то же значение — правки нет, undefined убирает ключ", () => {
    const text = motionsTextWithSetting(RUN, "run", "speed", 1.5);

    expect(motionsTextWithSetting(text, "run", "speed", 1.5)).toBeNull();
    expect(parsed(motionsTextWithSetting(text, "run", "speed", undefined)).run).toEqual({ model: "hero", clip: "Jog" });
    expect(motionsTextWithSetting(RUN, "run", "upper", undefined)).toBeNull();
  });

  it("loop и upper пишутся флажком и именем", () => {
    const text = motionsTextWithSetting(motionsTextWithSetting(RUN, "run", "loop", false), "run", "upper", "Idle");

    expect(parsed(text).run).toEqual({ model: "hero", clip: "Jog", loop: false, upper: "Idle" });
  });

  it("концы строк файла сохраняются: CRLF остаётся CRLF", () => {
    const text = '{\r\n  "run": { "model": "hero", "clip": "Jog" }\r\n}\r\n';

    const next = motionsTextWithSetting(text, "run", "bounce", 0) as string;

    expect(next).toContain("\r\n");
    expect(next.replace(/\r\n/g, "")).not.toContain("\n");
  });
});

describe("записанный файл", () => {
  it("записи — по строкам, тройки и отметки — в строку", () => {
    const text = motionsTextWithBoneTurn(motionsTextWithMark(RUN, "run", "Hips", 0.25, true), "run", "Hips", 0.25, [0, 0, 7]) as string;

    expect(text).toBe('{\n  "run": {\n    "model": "hero",\n    "clip": "Jog",\n    "bones": {\n      "Hips": [\n        { "at": 0.25, "turn": [0, 0, 7] }\n      ]\n    }\n  }\n}\n');
  });

  it("пустая таблица записывается как есть", () => {
    expect(formatMotionsText({}, null)).toBe("{}\n");
  });
});

describe("поворот кости", () => {
  it("без отметок ползунок пишет постоянную поправку", () => {
    const text = motionsTextWithBoneTurn(RUN, "run", "Hips", 0.4, [0, 12.345, 0]);

    expect(parsed(text).run?.bones).toEqual({ Hips: [0, 12.35, 0] });
  });

  it("при отметках правит отметку в текущей доле, а если её там нет — ставит новую", () => {
    let text = motionsTextWithMark(RUN, "run", "Hips", 0.25, true);
    text = motionsTextWithBoneTurn(text, "run", "Hips", 0.25, [0, 0, 10]);
    expect(readBoneCorrections(text, "run").Hips).toEqual(marks([0.25, 10]));

    text = motionsTextWithBoneTurn(text, "run", "Hips", 0.75, [0, 0, -10]);
    expect(readBoneCorrections(text, "run").Hips).toEqual(marks([0.25, 10], [0.75, -10]));

    text = motionsTextWithBoneTurn(text, "run", "Hips", 0.7501, [0, 0, -20]);
    expect(readBoneCorrections(text, "run").Hips).toEqual(marks([0.25, 10], [0.75, -20]));
  });

  it("отметка встаёт с тем поворотом, какой кость имеет в этой доле, — поза не прыгает", () => {
    let text = motionsTextWithBoneTurn(RUN, "run", "Hips", 0, [0, 0, 30]);
    text = motionsTextWithMark(text, "run", "Hips", 0.5, true);

    expect(readBoneCorrections(text, "run").Hips).toEqual(marks([0.5, 30]));
    const next = motionsTextWithMark(text, "run", "Hips", 0.75, true);
    expect(readBoneCorrections(next, "run").Hips).toEqual(marks([0.5, 30], [0.75, 30]));
  });

  it("вторая отметка в той же доле — правки нет", () => {
    const text = motionsTextWithMark(RUN, "run", "Hips", 0.25, true);

    expect(motionsTextWithMark(text, "run", "Hips", 0.2505, true)).toBeNull();
    expect(markIndexAt(marks([0.25, 0]), 0.2505)).toBe(0);
    expect(markIndexAt(marks([0.25, 0]), 0.5)).toBe(-1);
  });
});

describe("отметки: убрать, перетащить, сбросить", () => {
  const TWO_MARKS = motionsTextWithBoneTurn(motionsTextWithMark(motionsTextWithMark(RUN, "run", "Hips", 0.25, true), "run", "Hips", 0.75, true), "run", "Hips", 0.75, [0, 0, 9]);

  it("убранная отметка уходит со шкалы; последняя становится постоянной поправкой с её поворотом", () => {
    const one = motionsTextWithoutMark(TWO_MARKS, "run", "Hips", 0);
    expect(readBoneCorrections(one, "run").Hips).toEqual(marks([0.75, 9]));

    const none = motionsTextWithoutMark(one, "run", "Hips", 0);
    expect(readBoneCorrections(none, "run").Hips).toEqual([0, 0, 9]);
    expect(motionsTextWithoutMark(none, "run", "Hips", 0)).toBeNull();
  });

  it("перетащенная отметка меняет долю; к соседней вплотную её не поставить", () => {
    const moved = motionsTextWithMovedMark(TWO_MARKS, "run", "Hips", 0, 0.4);
    expect(readBoneCorrections(moved, "run").Hips).toEqual(marks([0.4, 0], [0.75, 9]));

    expect(motionsTextWithMovedMark(TWO_MARKS, "run", "Hips", 0, 0.7501)).toBeNull();
    expect(motionsTextWithMovedMark(TWO_MARKS, "run", "Hips", 5, 0.4)).toBeNull();
  });

  it("сброс кости снимает её поправки, сброс всех — ключ bones целиком", () => {
    let text = motionsTextWithBoneTurn(TWO_MARKS, "run", "Spine", 0, [1, 0, 0]);

    text = motionsTextWithoutBones(text, "run", "Hips");
    expect(Object.keys(parsed(text).run?.bones as object)).toEqual(["Spine"]);
    expect(motionsTextWithoutBones(text, "run", "Hips")).toBeNull();

    text = motionsTextWithoutBones(text, "run");
    expect(parsed(text).run).toEqual({ model: "hero", clip: "Jog" });
    expect(motionsTextWithoutBones(text, "run")).toBeNull();
  });
});

describe("поправка между отметками — как её считает движок", () => {
  const KEYED = marks([0.25, 10], [0.5, 40], [0.75, 20]);

  it("проходит через отметки и держит крайние, когда движение играет один раз", () => {
    for (const [at, z] of [[0.25, 10], [0.5, 40], [0.75, 20]] as const) {
      expect(effectiveBoneTurn(KEYED, at, false)[2]).toBeCloseTo(z, 4);
      expect(effectiveBoneTurn(KEYED, at, true)[2]).toBeCloseTo(z, 4);
    }
    expect(effectiveBoneTurn(KEYED, 0, false)[2]).toBe(10);
    expect(effectiveBoneTurn(KEYED, 1, false)[2]).toBe(20);
    expect(effectiveBoneTurn([1, 2, 3], 0.7, true)).toEqual([1, 2, 3]);
    expect(effectiveBoneTurn(marks([0.4, 7]), 0.9, true)[2]).toBe(7);
    expect(effectiveBoneTurn(undefined, 0.3, true)).toEqual([0, 0, 0]);
  });

  it("у повторяющегося движения после последней отметки идёт по кругу к первой", () => {
    const mirrored = marks([0.25, 40], [0.75, -40]);
    const turn = (phase: number): number => effectiveBoneTurn(mirrored, phase, true)[2];

    expect(turn(0)).toBeCloseTo(turn(1), 4);
    expect(turn(0)).toBeCloseTo(0, 4);
    expect(turn(0.1)).toBeGreaterThan(0);
    expect(turn(0.1) + turn(0.9)).toBeCloseTo(0, 4);
    expect(effectiveBoneTurn(mirrored, 0, false)[2]).toBeGreaterThan(39.9);
  });
});
