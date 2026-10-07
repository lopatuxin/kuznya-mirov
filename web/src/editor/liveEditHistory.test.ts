import { describe, expect, it } from "vitest";
import type { WorldObjectSummary } from "./battleTypes";
import { createLiveEditHistory, isLiveEditTargetAlive, popLiveEdit, pushLiveEdit, undoLiveTransform, undoLiveWind, type LiveEditEntry } from "./liveEditHistory";

describe("liveEditHistory", () => {
  it("пустая история — отмена недоступна", () => {
    expect(popLiveEdit(createLiveEditHistory())).toBe(null);
  });

  it("кладёт и снимает последнюю запись — требование 21", () => {
    let history = createLiveEditHistory();
    const first: LiveEditEntry = { kind: "set", id: 4, generation: 1, key: "velocity", hadKey: true, previous: [0, 0] };
    const second: LiveEditEntry = { kind: "delete", id: 4, properties: { position: [1, 1] } };
    history = pushLiveEdit(history, first);
    history = pushLiveEdit(history, second);

    const popped = popLiveEdit(history);
    expect(popped?.entry).toBe(second);
    expect(popped?.rest).toEqual([first]);
  });
});

describe("isLiveEditTargetAlive", () => {
  const WORLD: WorldObjectSummary[] = [{ id: 4, generation: 2, name: null }];

  it("объект жив под той же меткой — цела", () => {
    const entry: LiveEditEntry = { kind: "set", id: 4, generation: 2, key: "velocity", hadKey: true, previous: [0, 0] };
    expect(isLiveEditTargetAlive(entry, WORLD)).toBe(true);
  });

  it("правило удалило объект и номер занял новый с другой меткой — не цела (крайний случай)", () => {
    const entry: LiveEditEntry = { kind: "set", id: 4, generation: 1, key: "velocity", hadKey: true, previous: [0, 0] };
    expect(isLiveEditTargetAlive(entry, WORLD)).toBe(false);
  });

  it("номера больше нет в мире — не цела", () => {
    const entry: LiveEditEntry = { kind: "move", id: 9, generation: 1, previous: [0, 0] };
    expect(isLiveEditTargetAlive(entry, WORLD)).toBe(false);
  });

  it("копия (add) с чужой меткой на том же номере — не цела", () => {
    const entry: LiveEditEntry = { kind: "add", id: 4, generation: 1 };
    expect(isLiveEditTargetAlive(entry, WORLD)).toBe(false);
  });

  it("delete — всегда цела: объекта и не должно быть, отмена его восстанавливает", () => {
    const entry: LiveEditEntry = { kind: "delete", id: 4, properties: {} };
    expect(isLiveEditTargetAlive(entry, [])).toBe(true);
  });
});

describe("запись «transform» — жест ручки на паузе", () => {
  const entry = {
    kind: "transform",
    id: 4,
    generation: 2,
    changes: [
      { key: "position", hadKey: true, previous: [3, 4] },
      { key: "size", hadKey: true, previous: [2, 2] },
      { key: "rotation", hadKey: false, previous: undefined },
    ],
  } satisfies LiveEditEntry;

  it("отмена возвращает прежние значения одним проходом и убирает свойство, которого не было", () => {
    const calls: string[] = [];
    undoLiveTransform(entry, {
      set_property: (id, key, value) => calls.push(`set ${id} ${key} ${JSON.stringify(value)}`),
      remove_property: (id, key) => calls.push(`remove ${id} ${key}`),
    });
    expect(calls).toEqual(["set 4 position [3,4]", "set 4 size [2,2]", "remove 4 rotation"]);
  });

  it("объект под другой меткой жизни — запись не цела", () => {
    expect(isLiveEditTargetAlive(entry, [{ id: 4, generation: 2, name: null }])).toBe(true);
    expect(isLiveEditTargetAlive(entry, [{ id: 4, generation: 3, name: null }])).toBe(false);
  });
});

describe("правка ветра на ходу", () => {
  const WIND_EDIT: LiveEditEntry = { kind: "wind", previous: [1.5, 0], next: [-2, 0] };

  it("запись ветра цела при любом составе мира: к объекту она не привязана", () => {
    expect(isLiveEditTargetAlive(WIND_EDIT, [])).toBe(true);
  });

  it("отмена ставит прежний ветер тем же вызовом движка и файл не трогает", () => {
    const calls: unknown[] = [];
    undoLiveWind({ previous: [1.5, 0] }, { set_wind_particles: (settings) => (calls.push(settings), { ok: true }) });
    expect(calls).toEqual([{ wind: [1.5, 0] }]);
  });
});
