import { describe, expect, it } from "vitest";
import { withErrorLabels } from "./errorLabels";

const BASE = { file: "scene.json", path: "objects → 0 → run_speed", line: null, column: null };
const labelled = (message: string) => withErrorLabels({ ...BASE, message }).message;

describe("withErrorLabels", () => {
  it("ключ в тексте ошибки — русская подпись, место в файле прежнее", () => {
    const error = withErrorLabels({ ...BASE, message: 'свойство "run_speed" не может быть меньше нуля, получено -1' });

    expect(error.message).toBe('свойство "скорость бега" не может быть меньше нуля, получено -1');
    expect(error.path).toBe(BASE.path);
  });

  it("не хватает свойства в кавычках — подпись вместо ключа", () => {
    expect(labelled('объекту «герой» не хватает свойства "run_speed" для этого правила')).toBe(
      'объекту «герой» не хватает свойства "скорость бега" для этого правила',
    );
    expect(labelled('объекту «машина» не хватает свойства "drive_speed" для этого правила')).toBe('объекту «машина» не хватает свойства "скорость езды" для этого правила');
  });

  // Тексты — как их пишут engine/src/data/load.rs и engine/src/core/code.rs: подпись не ломает фразу.
  it("каждое сообщение движка про бег и прыжок читается по-русски", () => {
    expect(labelled('свойство "jump_height" должно быть больше нуля, получено 0')).toBe('свойство "высота прыжка" должно быть больше нуля, получено 0');
    expect(labelled('свойство "jump_time" должно быть больше нуля, получено -0.4')).toBe('свойство "время подъёма" должно быть больше нуля, получено -0.4');
    expect(labelled('свойство "run_speed" не может быть меньше нуля')).toBe('свойство "скорость бега" не может быть меньше нуля');
    expect(labelled('признак "on_ground" пишет только движок: его нельзя задать ни в файле, ни клавишей, ни правилом')).toBe(
      'признак "стоит на земле" пишет только движок: его нельзя задать ни в файле, ни клавишей, ни правилом',
    );
    expect(labelled('признак "on_ground" пишет только движок: коду он только для чтения')).toBe(
      'признак "стоит на земле" пишет только движок: коду он только для чтения',
    );
  });

  it("каждое свойство дорог и езды получает свою подпись", () => {
    expect(labelled('свойство "road_speed" не может быть меньше нуля, получено -2')).toBe('свойство "скорость на дороге" не может быть меньше нуля, получено -2');
    expect(labelled('признак "road" должен быть true или false')).toBe('признак "дорога" должен быть true или false');
    expect(labelled('свойство "drive_to" должно быть парой чисел')).toBe('свойство "куда ехать" должно быть парой чисел');
  });

  it("model и height в кавычках — «модель» и «высота»", () => {
    expect(labelled('свойство "model" должно быть текстом')).toBe('свойство "модель" должно быть текстом');
    expect(labelled('свойство "height" должно быть больше нуля, получено -1')).toBe('свойство "высота" должно быть больше нуля, получено -1');
    expect(labelled('shape задан объекту со свойством "model": модель рисуется своими материалами')).toBe(
      'shape задан объекту со свойством "модель": модель рисуется своими материалами',
    );
  });

  it("time_scale, row_height и sort_by в кавычках получают подписи", () => {
    expect(labelled('свойство "time_scale" должно быть от 0 до 16, получено 20')).toBe('свойство "скорость времени" должно быть от 0 до 16, получено 20');
    expect(labelled('свойство "row_height" должно быть больше нуля')).toBe('свойство "высота строки" должно быть больше нуля');
    expect(labelled('свойство "sort_by" называет свойство другого вида')).toBe('свойство "порядок по" называет свойство другого вида');
  });

  it("ключ files.models — «ключ models»", () => {
    expect(labelled("files.models есть только в трёхмерной сцене: у scene в game.json нет camera")).toBe("ключ models есть только в трёхмерной сцене: у scene в game.json нет camera");
    expect(labelled("files.models — ожидалась таблица")).toBe("ключ models — ожидалась таблица");
  });

  it("ключ без кавычек не трогает: подпись встала бы в фразу вроде «объекту с модель»", () => {
    const message = "model разрешён только объекту с position, а height должен быть больше нуля; row_height, sort_by, run_speed, road без кавычек";

    expect(labelled(message)).toBe(message);
  });

  it("слова с похожим началом и концом, имена через подчёркивание и точку не трогает", () => {
    const message =
      'models, "model_sizes", my_model, "view_height", sun_height, heights, object.height, modeling, "models", "my_run_speed", run_speed_max, "roads", "time_scale_x"';

    expect(labelled(message)).toBe(message);
  });

  it("русские фразы движка про модели остаются как есть", () => {
    const message = 'модели "house" нет; объявлены "birch"';

    expect(labelled(message)).toBe(message);
  });
});
