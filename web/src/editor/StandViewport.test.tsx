// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFakeBattleEngine, type FakeBattleEngine } from "./fakeBattleEngine";
import { readBoneCorrections } from "./motionsFile";
import { StandViewport } from "./StandViewport";
import { useAnimationStand } from "./useAnimationStand";

vi.mock("engine", () => ({
  // Поворот кости вокруг оси после кольца: поправка по оси z растёт на столько градусов, на сколько повёрнуто кольцо.
  motion_turn_after: (x: number, y: number, z: number, _axis: number, degrees: number) => [x, y, z + degrees],
}));

const RUN = '{ "run": { "model": "hero", "clip": "Jog" } }';

type HarnessProps = { engine: FakeBattleEngine; initialText: string; onText: (text: string) => void; isProjectLoaded?: boolean };

function Harness({ engine, initialText, onText, isProjectLoaded = true }: HarnessProps): React.JSX.Element {
  const [motionsText, setMotionsText] = useState(initialText);
  const canvasRef = useRef<HTMLCanvasElement>(document.createElement("canvas"));
  const stand = useAnimationStand({
    engine,
    isOpen: true,
    isProjectLoaded,
    motionsText,
    editMotions: (build) => {
      const next = build(motionsText);
      if (next === null) return;
      setMotionsText(next);
      onText(next);
    },
    createMotion: vi.fn(),
    setReloadGateOpen: vi.fn(),
    loadVersion: null,
  });
  const boneName = stand.selectedBone === null ? undefined : stand.info?.bones[stand.selectedBone]?.name;
  return (
    <>
      <output aria-label="кость">{boneName ?? "нет"}</output>
      <StandViewport stand={stand} worldCanvasRef={canvasRef} boneName={boneName} projectErrorLines={[]} isEditable />
    </>
  );
}

function overlay(): HTMLCanvasElement {
  return document.querySelector(".stand-viewport__overlay") as HTMLCanvasElement;
}

describe("просмотр экрана «Анимация»: мышь над моделью", () => {
  let engine: FakeBattleEngine;
  let onText: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", () => 0);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    HTMLElement.prototype.setPointerCapture = vi.fn();
    // Кадровый цикл рисует на холсте; в тесте рисовать нечем, и он не заводится.
    HTMLCanvasElement.prototype.getContext = vi.fn(() => null) as unknown as HTMLCanvasElement["getContext"];
    engine = createFakeBattleEngine();
    onText = vi.fn();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  function open(text = RUN, isProjectLoaded = true): void {
    render(<Harness engine={engine} initialText={text} onText={onText} isProjectLoaded={isProjectLoaded} />);
  }

  it("движок собрал стенд из текста файла: кнопки видов ставят камеру", () => {
    open();
    expect(engine.show_motion).toHaveBeenCalledWith(RUN, "run", "game");

    fireEvent.click(screen.getByRole("button", { name: "Спереди" }));
    expect(engine.motion_view).toHaveBeenLastCalledWith("front");
    fireEvent.click(screen.getByRole("button", { name: "Сбоку" }));
    expect(engine.motion_view).toHaveBeenLastCalledWith("side");
    expect(screen.getByRole("button", { name: "Сбоку" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("щелчок по модели выбирает кость под точкой, щелчок мимо снимает выбор", () => {
    vi.mocked(engine.motion_bone_at).mockReturnValue(1);
    open();

    fireEvent.pointerDown(overlay(), { button: 0, clientX: 40, clientY: 50, pointerId: 1 });
    expect(engine.motion_bone_at).toHaveBeenCalledWith(40, 50);
    expect(screen.getByLabelText("кость").textContent).toBe("Spine");

    vi.mocked(engine.motion_bone_at).mockReturnValue(undefined);
    fireEvent.pointerDown(overlay(), { button: 0, clientX: 400, clientY: 400, pointerId: 1 });
    expect(screen.getByLabelText("кость").textContent).toBe("нет");
  });

  it("колесо приближает, правая кнопка вращает вид, меню браузера не открывается", () => {
    open();

    fireEvent.wheel(overlay(), { deltaY: 100 });
    expect(engine.motion_zoom).toHaveBeenCalledWith(100);

    fireEvent.pointerDown(overlay(), { button: 2, clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerMove(overlay(), { clientX: 25, clientY: 4, pointerId: 1 });
    expect(engine.motion_turn).toHaveBeenCalledWith(15, -6);
    fireEvent.pointerUp(overlay(), { button: 2, pointerId: 1 });
    expect(fireEvent.contextMenu(overlay())).toBe(false);
  });

  it("кольцо тянется шагами: стенд пересобирается с поворотом кости, запись — при отпускании, одна", () => {
    vi.mocked(engine.motion_bone_at).mockReturnValue(1);
    vi.mocked(engine.motion_ring_at).mockReturnValue(2);
    vi.mocked(engine.motion_ring_drag).mockReturnValueOnce(10).mockReturnValueOnce(8);
    open();
    fireEvent.pointerDown(overlay(), { button: 0, clientX: 40, clientY: 50, pointerId: 1 });
    expect(onText).not.toHaveBeenCalled();

    fireEvent.pointerDown(overlay(), { button: 0, clientX: 60, clientY: 70, pointerId: 1 });
    expect(engine.motion_ring_grab).toHaveBeenLastCalledWith(1, 2, 60, 70);

    fireEvent.pointerMove(overlay(), { clientX: 62, clientY: 71, pointerId: 1 });
    const previewed = vi.mocked(engine.show_motion).mock.calls.at(-1)?.[0] as string;
    expect(readBoneCorrections(previewed, "run").Spine).toEqual([0, 0, 10]);
    // Стенд собран заново и хват потерян: кольцо хватается там, где указатель сейчас.
    expect(engine.motion_ring_grab).toHaveBeenLastCalledWith(1, 2, 62, 71);
    expect(onText).not.toHaveBeenCalled();

    fireEvent.pointerMove(overlay(), { clientX: 64, clientY: 72, pointerId: 1 });
    expect(readBoneCorrections(vi.mocked(engine.show_motion).mock.calls.at(-1)?.[0] as string, "run").Spine).toEqual([0, 0, 18]);

    fireEvent.pointerUp(overlay(), { button: 0, pointerId: 1 });
    expect(engine.motion_ring_release).toHaveBeenCalledTimes(1);
    expect(onText).toHaveBeenCalledTimes(1);
    expect(readBoneCorrections(onText.mock.calls[0]?.[0] as string, "run").Spine).toEqual([0, 0, 18]);
  });

  it("кольцо отпущено там же, где схвачено, — файл не пишется", () => {
    vi.mocked(engine.motion_bone_at).mockReturnValue(1);
    vi.mocked(engine.motion_ring_at).mockReturnValue(0);
    open();
    fireEvent.pointerDown(overlay(), { button: 0, clientX: 40, clientY: 50, pointerId: 1 });
    fireEvent.pointerDown(overlay(), { button: 0, clientX: 60, clientY: 70, pointerId: 1 });

    fireEvent.pointerUp(overlay(), { button: 0, pointerId: 1 });

    expect(onText).not.toHaveBeenCalled();
  });

  it("ошибка проверки видна текстом, пока просмотр показывает последнюю верную позу", () => {
    vi.mocked(engine.show_motion).mockReturnValue({ ok: false, error: "кости \"Nose\" нет в скелете" });
    open();

    expect(screen.getByRole("alert").textContent).toContain("кости \"Nose\" нет в скелете");
  });

  it("проект не загружен из-за ошибки: стенд не пересобирается и своей ошибки не пишет, шкала без чисел", () => {
    open(RUN, false);

    expect(engine.show_motion).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Движение собирается…")).toBeTruthy();
    expect(screen.getAllByText("—")).toHaveLength(2);
  });
});
