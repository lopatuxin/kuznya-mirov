import { describe, expect, it } from "vitest";
import { withRunJumpLabels } from "./runJumpLabels";

const BASE = { file: "scene.json", path: "objects → 0 → run_speed", line: null, column: null };
const labelled = (message: string) => withRunJumpLabels({ ...BASE, message }).message;

describe("withRunJumpLabels", () => {
  it("run_speed в тексте ошибки — «скорость бега», место в файле прежнее", () => {
    const error = withRunJumpLabels({ ...BASE, message: 'свойство "run_speed" не может быть меньше нуля, получено -1' });

    expect(error.message).toBe('свойство "скорость бега" не может быть меньше нуля, получено -1');
    expect(error.path).toBe(BASE.path);
  });

  it("не хватает свойства в кавычках — подпись вместо ключа", () => {
    expect(labelled('объекту «герой» не хватает свойства "run_speed" для этого правила')).toBe(
      'объекту «герой» не хватает свойства "скорость бега" для этого правила',
    );
  });

  // Тексты — как их пишут engine/src/data/load.rs и engine/src/core/code.rs: подпись не ломает фразу.
  it("каждое сообщение движка про бег и прыжок читается по-русски", () => {
    expect(labelled('свойство "jump_height" должно быть больше нуля, получено 0')).toBe('свойство "высота прыжка" должно быть больше нуля, получено 0');
    expect(labelled('свойство "jump_time" должно быть больше нуля, получено -0.4')).toBe('свойство "время подъёма" должно быть больше нуля, получено -0.4');
    expect(labelled('свойство "jump_time" должно быть больше нуля')).toBe('свойство "время подъёма" должно быть больше нуля');
    expect(labelled('свойство "run_speed" не может быть меньше нуля')).toBe('свойство "скорость бега" не может быть меньше нуля');
    expect(labelled('признак "on_ground" пишет только движок: его нельзя задать ни в файле, ни клавишей, ни правилом')).toBe(
      'признак "стоит на земле" пишет только движок: его нельзя задать ни в файле, ни клавишей, ни правилом',
    );
    expect(labelled('признак "on_ground" пишет только движок: коду он только для чтения')).toBe(
      'признак "стоит на земле" пишет только движок: коду он только для чтения',
    );
  });

  it("ключ без кавычек и чужие ключи с похожим началом не трогает", () => {
    const message = 'свойство "my_run_speed" и run_speed_max заданы, run_speed без кавычек';

    expect(labelled(message)).toBe(message);
  });
});
