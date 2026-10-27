// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { motionsTextWithBoneTurn, motionsTextWithMark, readBoneCorrections, readMotionNames, readMotionSettings } from "./motionsFile";
import { StandMotionList, newMotionProblem, suggestedMotionName } from "./StandMotionList";
import { StandSidePanel } from "./StandSidePanel";
import { StandTimeline } from "./StandTimeline";
import { createStandPlayback } from "./standPlayback";
import type { AnimationStand, MotionsChange } from "./useAnimationStand";

const RUN = '{ "run": { "model": "hero", "clip": "Jog" }, "idle": { "model": "hero", "clip": "Idle" } }';
const BONES = [
  { name: "mixamorig:Hips", parent: -1 },
  { name: "mixamorig:Spine", parent: 0 },
];
const HERO = { name: "hero", hasSkeleton: true, clips: ["Idle", "Jog", "Sprint"] };

type FakeStand = AnimationStand & { preview: ReturnType<typeof vi.fn>; commit: ReturnType<typeof vi.fn>; previewNow: ReturnType<typeof vi.fn>; setGestureActive: ReturnType<typeof vi.fn> };

function createStand(text: string, overrides: Partial<AnimationStand> = {}): FakeStand {
  const stand: AnimationStand = {
    engine: null,
    playback: createStandPlayback(),
    motionNames: readMotionNames(text),
    selectedName: "run",
    selectMotion: vi.fn(),
    createMotion: vi.fn(),
    settings: readMotionSettings(text, "run"),
    corrections: readBoneCorrections(text, "run"),
    isCyclic: true,
    view: "game",
    setView: vi.fn(),
    info: { bones: BONES, length: 0.8, frames: 24 },
    error: null,
    analysis: { slips: [], endGap: null },
    selectedBone: null,
    selectBone: vi.fn(),
    preview: vi.fn(),
    previewNow: vi.fn(),
    commit: vi.fn(),
    setGestureActive: vi.fn(),
    ...overrides,
  };
  return stand as FakeStand;
}

/** Правка, что стенд получил, применённая к тексту файла. */
function appliedChange(call: unknown[] | undefined, text: string): string | null {
  return (call?.[0] as MotionsChange)(text, "run");
}

afterEach(cleanup);

describe("новое движение в левой колонке", () => {
  const props = { motionsText: RUN, motionNames: ["run", "idle"], selectedName: "run", rigs: [HERO], canEdit: true, onSelect: vi.fn(), onCreate: vi.fn() };

  it("список показывает движения с моделью и движением файла, щелчок выбирает", () => {
    const onSelect = vi.fn();
    render(<StandMotionList {...props} onSelect={onSelect} />);

    const items = screen.getAllByRole("button").filter((button) => button.classList.contains("stand-list__item"));
    expect(items.map((item) => item.textContent)).toEqual(["runhero · Jog", "idlehero · Idle"]);
    fireEvent.click(items[1] as HTMLElement);
    expect(onSelect).toHaveBeenCalledWith("idle");
  });

  it("форма предлагает имя из модели и движения; «Создать» отдаёт имя, модель и движение файла", () => {
    const onCreate = vi.fn();
    render(<StandMotionList {...props} onCreate={onCreate} />);

    fireEvent.click(screen.getByRole("button", { name: "Новое движение" }));
    fireEvent.change(screen.getByLabelText("Движение файла"), { target: { value: "Sprint" } });
    expect((screen.getByLabelText("Имя") as HTMLInputElement).value).toBe("hero_sprint");
    fireEvent.click(screen.getByRole("button", { name: "Создать" }));

    expect(onCreate).toHaveBeenCalledWith("hero_sprint", "hero", "Sprint");
  });

  it("пустое и занятое имя — «Создать» недоступно, причина видна рядом", () => {
    render(<StandMotionList {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Новое движение" }));
    const create = screen.getByRole("button", { name: "Создать" }) as HTMLButtonElement;

    fireEvent.change(screen.getByLabelText("Имя"), { target: { value: "" } });
    expect(create.disabled).toBe(true);
    expect(screen.getByText("Введите имя нового движения")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Имя"), { target: { value: "run" } });
    expect(create.disabled).toBe(true);
    expect(screen.getByText("Это имя уже занято другим движением")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Имя"), { target: { value: "Jog" } });
    expect(screen.getByText("Это имя занято движением файла модели")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Имя"), { target: { value: "hero_new" } });
    expect(create.disabled).toBe(false);
  });

  it("без моделей со скелетом «Новое движение» недоступно", () => {
    render(<StandMotionList {...props} rigs={[]} />);

    const button = screen.getByRole("button", { name: "Новое движение" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toBe("В игре нет моделей со скелетом");
  });

  it("в файле нет движений — форма открыта сразу", () => {
    render(<StandMotionList {...props} motionsText={null} motionNames={[]} selectedName={null} />);

    expect(screen.getByRole("button", { name: "Создать" })).toBeTruthy();
  });

  it("имя занятое получает номер", () => {
    expect(suggestedMotionName("hero", "Jog", ["hero_jog"])).toBe("hero_jog_2");
    expect(newMotionProblem("x", [], undefined, "")).toBe("Выберите модель и движение файла");
  });
});

describe("правая колонка: ручки движения", () => {
  it("ползунок lean показывает на стенде при движении и пишет при отпускании", () => {
    const stand = createStand(RUN);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);
    const lean = screen.getByLabelText("наклон корпуса");

    fireEvent.input(lean, { target: { value: "12" } });
    expect(stand.setGestureActive).toHaveBeenCalledWith(true);
    expect(JSON.parse(appliedChange(stand.preview.mock.calls[0], RUN) ?? "").run.lean).toBe(12);
    expect(stand.commit).not.toHaveBeenCalled();

    fireEvent.change(lean, { target: { value: "12" } });
    expect(stand.setGestureActive).toHaveBeenLastCalledWith(false);
    expect(JSON.parse(appliedChange(stand.commit.mock.calls[0], RUN) ?? "").run.lean).toBe(12);
  });

  it("ползунок, отпущенный на прежнем значении, ничего не пишет: стенд возвращается к файлу", () => {
    const stand = createStand(RUN);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);

    fireEvent.change(screen.getByLabelText("наклон корпуса"), { target: { value: "0" } });

    expect(appliedChange(stand.commit.mock.calls[0], RUN)).toBeNull();
  });

  it("число в поле принимается по Enter и пишется", () => {
    const stand = createStand(RUN);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);
    const field = screen.getByLabelText("скорость, число");

    fireEvent.change(field, { target: { value: "1,5" } });
    fireEvent.keyDown(field, { key: "Enter" });

    expect(JSON.parse(appliedChange(stand.commit.mock.calls[0], RUN) ?? "").run.speed).toBe(1.5);
  });

  it("upper выбирается из движений файла модели, «нет» его убирает; loop — флажок", () => {
    const text = '{ "run": { "model": "hero", "clip": "Jog", "upper": "Idle" } }';
    const stand = createStand(text);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);
    const upper = screen.getByDisplayValue("Idle") as HTMLSelectElement;

    expect(Array.from(upper.options).map((option) => option.value)).toEqual(["", "Idle", "Jog", "Sprint"]);
    fireEvent.change(upper, { target: { value: "" } });
    expect(JSON.parse(appliedChange(stand.commit.mock.calls[0], text) ?? "").run.upper).toBeUndefined();

    fireEvent.click(screen.getByRole("checkbox"));
    expect(JSON.parse(appliedChange(stand.commit.mock.calls[1], text) ?? "").run.loop).toBe(false);
  });

  it("выбранная кость: три ползунка поворота пишут постоянную поправку, «Отметка» ставит отметку, сброс снимает", () => {
    const stand = createStand(RUN, { selectedBone: 1 });
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);

    fireEvent.change(screen.getByLabelText("Поворот Y"), { target: { value: "30" } });
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[0], RUN) ?? "", "run")["mixamorig:Spine"]).toEqual([0, 30, 0]);

    fireEvent.click(screen.getByRole("button", { name: "Отметка" }));
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[1], RUN) ?? "", "run")["mixamorig:Spine"]).toEqual([{ at: 0, turn: [0, 0, 0] }]);
    expect((screen.getByRole("button", { name: "Сбросить кость" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("при отметках ползунок правит отметку в текущей доле, а «Убрать отметку» снимает её", () => {
    const text = motionsTextWithMark(RUN, "run", "mixamorig:Spine", 0.5, true) as string;
    const stand = createStand(text, { selectedBone: 1 });
    act(() => stand.playback.setPhase(0.5));
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);

    fireEvent.change(screen.getByLabelText("Поворот Z"), { target: { value: "20" } });
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[0], text) ?? "", "run")["mixamorig:Spine"]).toEqual([{ at: 0.5, turn: [0, 0, 20] }]);

    fireEvent.click(screen.getByRole("button", { name: "Убрать отметку" }));
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[1], text) ?? "", "run")["mixamorig:Spine"]).toEqual([0, 0, 0]);
  });

  it("«Сбросить все» снимает поправки всех костей", () => {
    const text = motionsTextWithBoneTurn(RUN, "run", "mixamorig:Hips", 0, [0, 0, 5]) as string;
    const stand = createStand(text);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable />);

    fireEvent.click(screen.getByRole("button", { name: "Сбросить все" }));

    expect(JSON.parse(appliedChange(stand.commit.mock.calls[0], text) ?? "").run.bones).toBeUndefined();
  });

  it("выбор кости прокручивает дерево к ней, даже когда браузер отвечает на прокрутку обещанием", () => {
    const scrollIntoView = vi.fn(() => Promise.resolve());
    HTMLElement.prototype.scrollIntoView = scrollIntoView as unknown as HTMLElement["scrollIntoView"];
    const { rerender } = render(<StandSidePanel stand={createStand(RUN, { selectedBone: 1 })} fileClips={HERO.clips} isEditable />);

    rerender(<StandSidePanel stand={createStand(RUN, { selectedBone: 0 })} fileClips={HERO.clips} isEditable />);

    expect(scrollIntoView).toHaveBeenCalled();
  });

  it("без правки файла (движений нет) ручки недоступны", () => {
    const stand = createStand(RUN);
    render(<StandSidePanel stand={stand} fileClips={HERO.clips} isEditable={false} />);

    expect((screen.getByLabelText("наклон корпуса") as HTMLInputElement).disabled).toBe(true);
  });
});

describe("шкала движения", () => {
  const WITH_MARKS = motionsTextWithBoneTurn(motionsTextWithMark(RUN, "run", "mixamorig:Spine", 0.25, true), "run", "mixamorig:Spine", 0.75, [0, 0, 9]) as string;

  it("пуск и пауза, шаг по кадру и доля со временем в секундах", () => {
    const stand = createStand(RUN);
    render(<StandTimeline stand={stand} boneName={undefined} isEditable />);

    fireEvent.click(screen.getByRole("button", { name: "Остановить движение" }));
    expect(stand.playback.getState().isPlaying).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Кадр вперёд" }));
    expect(stand.playback.getState().phase).toBeCloseTo(1 / 24, 6);
    expect(screen.getByText("доля 0.042")).toBeTruthy();
    expect(screen.getByText("0.03 с из 0.80 с")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Кадр назад" }));
    expect(stand.playback.getState().phase).toBe(0);
    expect(screen.getByRole("button", { name: "Играть движение" })).toBeTruthy();
  });

  it("отметки выбранной кости видны засечками, двойной щелчок и Delete убирают", () => {
    const stand = createStand(WITH_MARKS);
    render(<StandTimeline stand={stand} boneName="mixamorig:Spine" isEditable />);

    const marks = screen.getAllByRole("button", { name: /^Отметка/ });
    expect(marks.map((mark) => (mark as HTMLElement).style.left)).toEqual(["25%", "75%"]);

    fireEvent.doubleClick(marks[0] as HTMLElement);
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[0], WITH_MARKS) ?? "", "run")["mixamorig:Spine"]).toEqual([{ at: 0.75, turn: [0, 0, 9] }]);
    fireEvent.keyDown(marks[1] as HTMLElement, { key: "Delete" });
    expect(readBoneCorrections(appliedChange(stand.commit.mock.calls[1], WITH_MARKS) ?? "", "run")["mixamorig:Spine"]).toEqual([{ at: 0.25, turn: [0, 0, 0] }]);
  });

  it("у кости без отметок засечек нет", () => {
    const stand = createStand(RUN);
    render(<StandTimeline stand={stand} boneName="mixamorig:Spine" isEditable />);

    expect(screen.queryAllByRole("button", { name: /^Отметка/ })).toEqual([]);
  });

  it("красные метки: скольжение ступней и расхождение конца с началом", () => {
    const stand = createStand(RUN, { analysis: { slips: [{ foot: 0, from: 0.1, to: 0.3 }, { foot: 1, from: 0.9, to: 0.1 }], endGap: { degrees: 12.4, bone: "mixamorig:LeftArm" } } });
    const { container } = render(<StandTimeline stand={stand} boneName={undefined} isEditable />);

    expect(container.querySelectorAll(".stand-timeline__slip")).toHaveLength(3);
    expect(container.querySelector(".stand-timeline__gap")).not.toBeNull();
    expect(screen.getByText("конец расходится с началом на 12° (mixamorig:LeftArm)")).toBeTruthy();
    expect(screen.getByText("ступня скользит: 2")).toBeTruthy();
  });
});
