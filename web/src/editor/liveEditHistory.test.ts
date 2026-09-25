import { describe, expect, it } from "vitest";
import type { WorldObjectSummary } from "./battleTypes";
import { createLiveEditHistory, isLiveEditTargetAlive, popLiveEdit, pushLiveEdit, type LiveEditEntry } from "./liveEditHistory";

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
