import { describe, expect, it } from "vitest";
import { buildLiveObjectSummaries, buildLivePropertiesView, findWorldObject, liveObjectListEmptyLabel, resolveCanStartReplay, resolveLiveSelection } from "./battleSelection";
import type { WorldObjectSummary } from "./battleTypes";

const OBJECTS: WorldObjectSummary[] = [
  { id: 2, generation: 5, name: "мяч" },
  { id: 4, generation: 1, name: null },
];

describe("resolveLiveSelection", () => {
  it("объект жив под той же меткой — выбор остаётся", () => {
    expect(resolveLiveSelection({ id: 2, generation: 5 }, OBJECTS)).toEqual({ id: 2, generation: 5 });
  });

  it("номер занял новый объект с другой меткой — выбор снят (требование 15)", () => {
    expect(resolveLiveSelection({ id: 2, generation: 1 }, OBJECTS)).toBe(null);
  });

  it("объекта с таким номером больше нет — выбор снят", () => {
    expect(resolveLiveSelection({ id: 9, generation: 1 }, OBJECTS)).toBe(null);
  });

  it("выбора не было — остаётся null", () => {
    expect(resolveLiveSelection(null, OBJECTS)).toBe(null);
  });
});

describe("findWorldObject", () => {
  it("находит объект по номеру", () => {
    expect(findWorldObject(OBJECTS, 4)).toEqual({ id: 4, generation: 1, name: null });
  });

  it("номера нет — undefined", () => {
    expect(findWorldObject(OBJECTS, 99)).toBe(undefined);
  });
});

describe("buildLiveObjectSummaries", () => {
  it("номер и имя, по возрастанию как пришли из world_objects", () => {
    expect(buildLiveObjectSummaries(OBJECTS)).toEqual([
      { index: 2, name: "мяч", color: null, image: null, isOnScene: true },
      { index: 4, name: null, color: null, image: null, isOnScene: true },
    ]);
  });
});

describe("liveObjectListEmptyLabel", () => {
  it("мир есть — «Объектов нет»", () => {
    expect(liveObjectListEmptyLabel(true)).toBe("Объектов нет");
  });

  it("мира нет — «Мира нет» (требование 13)", () => {
    expect(liveObjectListEmptyLabel(false)).toBe("Мира нет");
  });
});

describe("resolveCanStartReplay", () => {
  it("запись есть, стоим в правке без ошибок, ничего не идёт — доступна", () => {
    expect(resolveCanStartReplay(true, "edit", false, true)).toBe(true);
  });

  it("записи нет — недоступна", () => {
    expect(resolveCanStartReplay(false, "edit", false, true)).toBe(false);
  });

  it("уже в повторе — недоступна", () => {
    expect(resolveCanStartReplay(true, "replay", false, true)).toBe(false);
  });

  it("партия идёт — недоступна", () => {
    expect(resolveCanStartReplay(true, "battle", true, true)).toBe(false);
  });

  it("проект сейчас не загружен без ошибок (неудачная перезагрузка уронила запись у движка) — недоступна, требование 28", () => {
    expect(resolveCanStartReplay(true, "edit", false, false)).toBe(false);
  });
});

describe("buildLivePropertiesView", () => {
  it("объекта нет — status none", () => {
    expect(buildLivePropertiesView(undefined)).toEqual({ status: "none" });
  });

  it("свойства объекта — по строке на ключ, значением JSON", () => {
    expect(buildLivePropertiesView({ position: [1, 2], color: "#e04040" })).toEqual({
      status: "object",
      properties: [
        { key: "position", value: [1, 2], valueText: "[1,2]" },
        { key: "color", value: "#e04040", valueText: '"#e04040"' },
      ],
    });
  });
});
