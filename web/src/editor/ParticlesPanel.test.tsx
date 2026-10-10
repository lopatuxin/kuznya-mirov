// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LEAVES_DRAG_TYPE, PARTICLES_DRAG_TYPE } from "./imageDrag";
import { LEAF_COLOR_START } from "./particleEffects";
import { ParticlesPanel } from "./ParticlesPanel";

type PanelProps = Parameters<typeof ParticlesPanel>[0];

const SMOKE_AND_LEAVES = { position: [1, 1], size: [2, 2], smoke: 0.5, leaf_fall: 0.3 };
const FIRE = { position: [1, 1], size: [2, 1], fire: 0.6 };
const SPARKS = { position: [1, 1], size: [1, 1], sparks: 0.5, sparks_reach: 2 };

function renderPanel(properties: PanelProps["properties"], overrides: Partial<PanelProps> = {}): { props: PanelProps; rerender: (next: PanelProps["properties"]) => void } {
  const props: PanelProps = {
    properties,
    isEditable: true,
    onPreview: vi.fn(() => undefined),
    onCommit: vi.fn(),
    onRemove: vi.fn(),
    onGestureActiveChange: vi.fn(),
    ...overrides,
  };
  const { rerender } = render(<ParticlesPanel {...props} />);
  return { props, rerender: (next) => rerender(<ParticlesPanel {...props} properties={next} />) };
}

function type(input: HTMLElement, text: string): void {
  act(() => input.focus());
  fireEvent.change(input, { target: { value: text } });
}

afterEach(cleanup);

describe("ParticlesPanel: карточки", () => {
  it("четыре карточки — дым, искры, листья, огонь, без «Копировать» и «Удалить»", () => {
    renderPanel(null);

    expect(within(screen.getByRole("list", { name: "Эффекты" })).getAllByRole("listitem").map((card) => card.textContent)).toEqual(["дым", "искры", "листья", "огонь"]);
    expect(screen.queryByRole("button", { name: "Копировать" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Удалить" })).toBeNull();
  });

  it("карточка при перетаскивании отдаёт ключ эффекта, листья — ещё и свой тип для рамки над целью", () => {
    renderPanel(null);
    const setData = vi.fn();
    const [smoke, , leaves] = screen.getAllByRole("listitem");

    fireEvent.dragStart(smoke as HTMLElement, { dataTransfer: { setData, effectAllowed: "" } });
    expect(setData).toHaveBeenCalledTimes(1);
    expect(setData).toHaveBeenCalledWith(PARTICLES_DRAG_TYPE, "smoke");

    setData.mockClear();
    fireEvent.dragStart(leaves as HTMLElement, { dataTransfer: { setData, effectAllowed: "" } });
    expect(setData).toHaveBeenCalledWith(PARTICLES_DRAG_TYPE, "leaves");
    expect(setData).toHaveBeenCalledWith(LEAVES_DRAG_TYPE, "leaves");
  });

  it("карточка «огонь» отдаёт свой ключ и не отдаёт тип листьев", () => {
    renderPanel(null);
    const setData = vi.fn();

    fireEvent.dragStart(screen.getAllByRole("listitem")[3] as HTMLElement, { dataTransfer: { setData, effectAllowed: "" } });

    expect(setData).toHaveBeenCalledTimes(1);
    expect(setData).toHaveBeenCalledWith(PARTICLES_DRAG_TYPE, "fire");
  });

  it("правка недоступна (повтор, проект с ошибками) — карточки не тянутся", () => {
    renderPanel(null, { isEditable: false });

    for (const card of screen.getAllByRole("listitem")) expect(card.getAttribute("draggable")).toBe("false");
  });
});

describe("ParticlesPanel: группы выбранного объекта", () => {
  it("объект с дымом и листопадом — группы «Дым» и «Листья» по порядку, искр нет", () => {
    renderPanel(SMOKE_AND_LEAVES);

    expect(screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual(["Дым", "Листья"]);
    expect(screen.queryByText(/Дым, искры и огонь перетащите/)).toBeNull();
  });

  it("объект без частиц и без выбора — подсказка", () => {
    renderPanel({ position: [1, 1], size: [1, 1] });
    expect(screen.getByText("Дым, искры и огонь перетащите туда, откуда они идут, — на трубу, костёр или горн. Листья — на дерево")).toBeTruthy();
    cleanup();

    renderPanel(null);
    expect(screen.getByText(/Дым, искры и огонь перетащите туда, откуда они идут/)).toBeTruthy();
  });

  it("плотность 0 группу не прячет: эффект остаётся, пока его не уберут", () => {
    renderPanel({ smoke: 0 });
    expect(screen.getByRole("region", { name: "Дым" })).toBeTruthy();
  });

  it("подписи полные русские слова: у слайдера концы, у полей единицы", () => {
    renderPanel({ smoke: 0.5, sparks: 0.5, leaf_fall: 0.3 });

    for (const text of ["струйка", "густой столб", "редкие", "густые", "изредка", "сильный", "высота столба, клеток", "как далеко летят, клеток", "направление и разброс", "цвет листьев", "осенние вперемешку"]) {
      expect(screen.getByText(text)).toBeTruthy();
    }
    expect(document.body.textContent).not.toMatch(/smoke|sparks|leaf_/);
  });

  it("значения, которых у объекта нет, показаны бледно умолчаниями", () => {
    renderPanel({ smoke: 0.5, sparks: 0.5 });

    const height = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;
    const reach = screen.getByLabelText("как далеко летят, клеток") as HTMLInputElement;
    expect(height.value).toBe("4");
    expect(height.className).toContain("particles-input--default");
    expect(reach.value).toBe("1,5");
    expect(screen.getByText("вверх, ±30°")).toBeTruthy();
  });

  it("значения объекта показаны обычно, число — по-русски", () => {
    renderPanel({ smoke: 0.5, smoke_height: 2.5, sparks: 0.5, sparks_direction: 90, sparks_spread: 45 });

    const height = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;
    expect(height.value).toBe("2,5");
    expect(height.className).not.toContain("particles-input--default");
    expect(screen.getByText("вправо, ±45°")).toBeTruthy();
  });
});

describe("ParticlesPanel: плотность", () => {
  it("движение ползунка ставит значение сцене на лету, запись — только при отпускании", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);
    const slider = screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement;

    fireEvent.input(slider, { target: { value: "0.7" } });
    fireEvent.input(slider, { target: { value: "0.95" } });

    expect(props.onPreview).toHaveBeenNthCalledWith(1, "smoke", 0.7);
    expect(props.onPreview).toHaveBeenNthCalledWith(2, "smoke", 0.95);
    expect(props.onCommit).not.toHaveBeenCalled();

    fireEvent.change(slider);

    expect(props.onCommit).toHaveBeenCalledTimes(1);
    expect(props.onCommit).toHaveBeenCalledWith("smoke", 0.95, 0.5);
  });

  it("после отпускания ползунок держит новое значение, пока объект не получит его, — не прыгает назад", () => {
    const { rerender } = renderPanel(SMOKE_AND_LEAVES);
    const slider = (): HTMLInputElement => screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement;

    fireEvent.input(slider(), { target: { value: "0.8" } });
    fireEvent.change(slider());
    expect(slider().value).toBe("0.8");

    rerender({ ...SMOKE_AND_LEAVES, smoke: 0.8 });
    expect(slider().value).toBe("0.8");
  });

  it("у листьев свой ползунок со своим ключом", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);
    const leavesSlider = within(screen.getByRole("region", { name: "Листья" })).getByRole("slider", { name: "плотность" });

    fireEvent.input(leavesSlider, { target: { value: "0.6" } });
    fireEvent.change(leavesSlider);

    expect(props.onCommit).toHaveBeenCalledWith("leaf_fall", 0.6, 0.3);
  });

  it("ползунок вернули на прежнее значение — запись не просят", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);
    const slider = screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement;

    fireEvent.input(slider, { target: { value: "0.9" } });
    fireEvent.input(slider, { target: { value: "0.5" } });
    fireEvent.change(slider);

    expect(props.onCommit).not.toHaveBeenCalled();
  });

  it("правка недоступна — ползунок и «Убрать» неактивны", () => {
    renderPanel(SMOKE_AND_LEAVES, { isEditable: false });

    expect((screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement).disabled).toBe(true);
    expect((screen.getAllByRole("button", { name: "Убрать" })[0] as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ParticlesPanel: число настройки", () => {
  it("набор ставит значение сцене, Enter принимает его с прежним значением для отмены", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "6,5");
    expect(props.onPreview).toHaveBeenLastCalledWith("smoke_height", 6.5);
    expect(props.onCommit).not.toHaveBeenCalled();

    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onCommit).toHaveBeenCalledWith("smoke_height", 6.5, 4);
  });

  it("уход из поля принимает значение так же, как Enter, и один раз", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "5");
    fireEvent.keyDown(input, { key: "Enter" });
    act(() => input.blur());

    expect(props.onCommit).toHaveBeenCalledTimes(1);
  });

  it("свойства не было — первая правка передаёт «не было», чтобы отмена в партии его сняла", () => {
    const { props } = renderPanel({ smoke: 0.5 });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "7");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(props.onCommit).toHaveBeenCalledWith("smoke_height", 7, undefined);
  });

  it("Esc возвращает прежнее значение сцене и ничего не пишет", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;

    type(input, "9");
    fireEvent.keyDown(input, { key: "Escape" });

    expect(props.onPreview).toHaveBeenLastCalledWith("smoke_height", 4);
    expect(props.onCommit).not.toHaveBeenCalled();
    expect(input.value).toBe("4");
  });

  it("то же число — запись не просят", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "4,0");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(props.onCommit).not.toHaveBeenCalled();
  });

  it("не число после Enter — прежнее значение, ничего не пишется", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;

    type(input, "много");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(props.onCommit).not.toHaveBeenCalled();
    expect(input.value).toBe("4");
  });

  it("движок не принял число — текст ошибки под полем, значение сцене возвращено, запись не просят", () => {
    const onPreview = vi.fn((key: string, value: unknown) => (key === "smoke_height" && value === 0 ? "smoke_height должно быть больше нуля, получено 0" : undefined));
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 }, { onPreview });
    const input = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;

    type(input, "0");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByRole("alert").textContent).toBe("высота столба, клеток должно быть больше нуля, получено 0");
    expect(props.onCommit).not.toHaveBeenCalled();
    expect(onPreview).toHaveBeenLastCalledWith("smoke_height", 4);
    expect(input.value).toBe("0");
  });

  it("число, которого у объекта не было, не бледное с первой цифры и после Enter, пока запись не дошла до свойств", () => {
    renderPanel({ smoke: 0.5 });
    const input = screen.getByLabelText("высота столба, клеток") as HTMLInputElement;
    expect(input.className).toContain("particles-input--default");

    type(input, "7");
    expect(input.className).not.toContain("particles-input--default");

    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.className).not.toContain("particles-input--default");
    expect(input.value).toBe("7");
  });

  it("в тексте ошибки ключи свойств частиц заменены русскими подписями, остальное слово в слово", () => {
    const onPreview = vi.fn((key: string) => (key === "sparks_reach" ? "sparks_reach: нужно больше нуля, получено 0; sparks не трогаем, sparks_spread и smoke_color тоже" : undefined));
    renderPanel(SPARKS, { onPreview });
    const input = screen.getByLabelText("как далеко летят, клеток");

    type(input, "0");

    expect(screen.getByRole("alert").textContent).toBe("как далеко летят, клеток: нужно больше нуля, получено 0; плотность не трогаем, разброс и цвет тоже");
  });
});

describe("ParticlesPanel: перечитывание файлов на время жеста", () => {
  it("число держит перечитывание от первой цифры до Enter; Esc тоже отпускает", () => {
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "6");
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);

    type(input, "7");
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    fireEvent.keyDown(input, { key: "Escape" });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });

  it("число, которое движок не принял, и не число после Enter перечитывание тоже отпускают", () => {
    const onPreview = vi.fn((_key: string, value: unknown) => (value === 0 ? "нельзя" : undefined));
    const { props } = renderPanel({ smoke: 0.5, smoke_height: 4 }, { onPreview });
    const input = screen.getByLabelText("высота столба, клеток");

    type(input, "0");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);

    type(input, "много");
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });

  it("ползунок держит перечитывание от первого движения до отпускания", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);
    const slider = screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement;

    expect(props.onGestureActiveChange).not.toHaveBeenCalled();
    fireEvent.input(slider, { target: { value: "0.7" } });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    fireEvent.change(slider);
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });

  it("круг держит перечитывание от нажатия на ручку до отпускания, мимо ручек — нет", () => {
    const { props } = renderPanel(SPARKS);
    const dial = screen.getByRole("group", { name: "направление и разброс искр" });

    fireEvent.pointerDown(dial, { pointerId: 1 });
    expect(props.onGestureActiveChange).not.toHaveBeenCalled();

    fireEvent.pointerDown(dial.querySelector("[data-part=direction]") as Element, { pointerId: 1 });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    fireEvent.pointerUp(dial, { pointerId: 1 });
    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });

  it("панель, ушедшая посреди жеста (другой объект, партия), отпускает перечитывание сама", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);
    fireEvent.input(screen.getAllByRole("slider", { name: "плотность" })[0] as HTMLInputElement, { target: { value: "0.7" } });

    cleanup();

    expect(props.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });

  it("панель без жеста при уходе перечитывания не трогает", () => {
    const { props } = renderPanel(SMOKE_AND_LEAVES);

    cleanup();

    expect(props.onGestureActiveChange).not.toHaveBeenCalled();
  });
});

describe("ParticlesPanel: цвет", () => {
  it("палитра дыма — серый по умолчанию бледно; выбранный цвет принимается как свойство", () => {
    const { props } = renderPanel({ smoke: 0.5 });
    const picker = screen.getByRole("region", { name: "Дым" }).querySelector("input[type=color]") as HTMLInputElement;

    expect(picker.value).toBe("#a6a6ac");
    expect(picker.closest(".particles-color--default")).not.toBeNull();

    picker.value = "#ff8800";
    fireEvent.change(picker);

    expect(props.onCommit).toHaveBeenCalledWith("smoke_color", "#ff8800", undefined);
  });

  it("листья по умолчанию осенние вперемешку: галочка стоит; снять её — цвет по умолчанию записывается", () => {
    const { props } = renderPanel({ leaf_fall: 0.3 });
    const autumn = screen.getByLabelText("осенние вперемешку") as HTMLInputElement;
    expect(autumn.checked).toBe(true);

    fireEvent.click(autumn);

    expect(props.onCommit).toHaveBeenCalledWith("leaf_color", LEAF_COLOR_START, undefined);
  });

  it("с цветом листьев галочка снята; поставить её — leaf_color снимается", () => {
    const { props } = renderPanel({ leaf_fall: 0.3, leaf_color: "#aa3300" });
    const autumn = screen.getByLabelText("осенние вперемешку") as HTMLInputElement;
    expect(autumn.checked).toBe(false);

    fireEvent.click(autumn);

    expect(props.onRemove).toHaveBeenCalledWith(["leaf_color"]);
  });
});

describe("ParticlesPanel: огонь", () => {
  function fireRegion(): HTMLElement {
    return screen.getByRole("region", { name: "Огонь" });
  }

  it("объект с fire — группа «Огонь» после остальных, главный ползунок называется «сила огня»", () => {
    renderPanel({ ...FIRE, smoke: 0.5, leaf_fall: 0.3 });

    expect(screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual(["Дым", "Листья", "Огонь"]);
    const strength = within(fireRegion()).getByRole("slider", { name: "сила огня" }) as HTMLInputElement;
    expect(strength.value).toBe("0.6");
    expect(strength.step).toBe("0.05");
    expect(within(fireRegion()).getByRole("slider", { name: "яркость ореола" })).toBeTruthy();
  });

  it("подписи полные русские слова: концы ползунков, цвет пламени, без английских ключей", () => {
    renderPanel(FIRE);

    for (const text of ["тлеет", "бушует", "без ореола", "яркий", "сила огня", "цвет пламени", "яркость ореола"]) {
      expect(within(fireRegion()).getByText(text)).toBeTruthy();
    }
    expect(fireRegion().textContent).not.toMatch(/fire/);
  });

  it("сила огня: на каждое движение сцена меняется, запись один раз — при отпускании, с прежним значением для отмены", () => {
    const { props } = renderPanel(FIRE);
    const slider = within(fireRegion()).getByRole("slider", { name: "сила огня" });

    fireEvent.input(slider, { target: { value: "0.2" } });
    fireEvent.input(slider, { target: { value: "0" } });

    expect(props.onPreview).toHaveBeenNthCalledWith(1, "fire", 0.2);
    expect(props.onPreview).toHaveBeenNthCalledWith(2, "fire", 0);
    expect(props.onCommit).not.toHaveBeenCalled();

    fireEvent.change(slider);

    expect(props.onCommit).toHaveBeenCalledTimes(1);
    expect(props.onCommit).toHaveBeenCalledWith("fire", 0, 0.6);
  });

  it("сила 0 группу не прячет: огонь остаётся в списке, пока его не уберут", () => {
    renderPanel({ ...FIRE, fire: 0 });
    expect(fireRegion()).toBeTruthy();
  });

  it("цвет пламени: без fire_color показан оранжевый бледно; выбранный цвет принимается как свойство", () => {
    const { props } = renderPanel(FIRE);
    const picker = fireRegion().querySelector("input[type=color]") as HTMLInputElement;

    expect(picker.value).toBe("#ff8c1a");
    expect(picker.closest(".particles-color--default")).not.toBeNull();

    picker.value = "#2255ff";
    fireEvent.change(picker);

    expect(props.onCommit).toHaveBeenCalledWith("fire_color", "#2255ff", undefined);
  });

  it("цвет пламени, который у объекта есть, показан обычно", () => {
    renderPanel({ ...FIRE, fire_color: "#2255ff" });
    const picker = fireRegion().querySelector("input[type=color]") as HTMLInputElement;

    expect(picker.value).toBe("#2255ff");
    expect(picker.closest(".particles-color--default")).toBeNull();
  });

  it("яркость ореола: без fire_glow стоит на 0,5 бледно; движение ставит значение сцене, отпускание пишет с «не было»", () => {
    const { props } = renderPanel(FIRE);
    const slider = within(fireRegion()).getByRole("slider", { name: "яркость ореола" }) as HTMLInputElement;

    expect(slider.value).toBe("0.5");
    expect(slider.closest(".particles-density--default")).not.toBeNull();

    fireEvent.input(slider, { target: { value: "0.8" } });
    expect(props.onPreview).toHaveBeenLastCalledWith("fire_glow", 0.8);
    expect(slider.closest(".particles-density--default")).toBeNull();
    expect(props.onCommit).not.toHaveBeenCalled();

    fireEvent.change(slider);
    expect(props.onCommit).toHaveBeenCalledTimes(1);
    expect(props.onCommit).toHaveBeenCalledWith("fire_glow", 0.8, undefined);
  });

  it("яркость ореола, которая у объекта есть, показана обычно и пишется с прежним значением", () => {
    const { props } = renderPanel({ ...FIRE, fire_glow: 0.3 });
    const slider = within(fireRegion()).getByRole("slider", { name: "яркость ореола" }) as HTMLInputElement;

    expect(slider.value).toBe("0.3");
    expect(slider.closest(".particles-density--default")).toBeNull();

    fireEvent.input(slider, { target: { value: "0" } });
    fireEvent.change(slider);

    expect(props.onCommit).toHaveBeenCalledWith("fire_glow", 0, 0.3);
  });

  it("ошибка движка про fire_glow стоит под ползунком с подписью «яркость ореола»", () => {
    const onPreview = vi.fn((key: string) => (key === "fire_glow" ? "fire_glow: нужно от 0 до 1 включительно, получено 2" : undefined));
    renderPanel(FIRE, { onPreview });

    fireEvent.input(within(fireRegion()).getByRole("slider", { name: "яркость ореола" }), { target: { value: "1" } });

    expect(screen.getByRole("alert").textContent).toBe("яркость ореола: нужно от 0 до 1 включительно, получено 2");
  });

  it("«Убрать» снимает fire, fire_color и fire_glow одной правкой и только их", () => {
    const { props } = renderPanel({ ...FIRE, fire_color: "#2255ff", fire_glow: 0.3, smoke: 0.5 });

    fireEvent.click(within(fireRegion()).getByRole("button", { name: "Убрать" }));

    expect(props.onRemove).toHaveBeenCalledTimes(1);
    expect(props.onRemove).toHaveBeenCalledWith(["fire", "fire_color", "fire_glow"]);
  });

  it("правка недоступна — ползунки и «Убрать» огня неактивны", () => {
    renderPanel(FIRE, { isEditable: false });

    expect((within(fireRegion()).getByRole("slider", { name: "сила огня" }) as HTMLInputElement).disabled).toBe(true);
    expect((within(fireRegion()).getByRole("slider", { name: "яркость ореола" }) as HTMLInputElement).disabled).toBe(true);
    expect((within(fireRegion()).getByRole("button", { name: "Убрать" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ParticlesPanel: «Убрать»", () => {
  it("отдаёт все свойства эффекта, которые есть у объекта, и только их", () => {
    const { props } = renderPanel({ ...SPARKS, sparks_spread: 60, smoke: 0.5 });

    fireEvent.click(within(screen.getByRole("region", { name: "Искры" })).getByRole("button", { name: "Убрать" }));

    expect(props.onRemove).toHaveBeenCalledWith(["sparks", "sparks_reach", "sparks_spread"]);
  });
});

describe("ParticlesPanel: круг направления и разброса", () => {
  const CENTER = 50;

  function renderDial(properties: PanelProps["properties"] = SPARKS): { props: PanelProps; dial: SVGSVGElement; part: (name: "direction" | "spread") => Element } {
    const { props } = renderPanel(properties);
    const dial = screen.getByRole("group", { name: "направление и разброс искр" }) as unknown as SVGSVGElement;
    Object.defineProperty(dial, "getBoundingClientRect", { value: () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) }) });
    return { props, dial, part: (name) => dial.querySelector(`[data-part=${name}]`) as Element };
  }

  function pointAtAngle(degrees: number, radius = 40): { clientX: number; clientY: number } {
    const radians = (degrees * Math.PI) / 180;
    return { clientX: CENTER + radius * Math.sin(radians), clientY: CENTER - radius * Math.cos(radians) };
  }

  it("конец стрелки тянут — меняется sparks_direction: на каждое движение сцена, при отпускании запись", () => {
    const { props, dial, part } = renderDial();

    fireEvent.pointerDown(part("direction"), { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(45) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(90) });

    expect(props.onPreview).toHaveBeenNthCalledWith(1, "sparks_direction", 45);
    expect(props.onPreview).toHaveBeenNthCalledWith(2, "sparks_direction", 90);
    expect(props.onCommit).not.toHaveBeenCalled();
    expect(screen.getByText("вправо, ±30°")).toBeTruthy();

    fireEvent.pointerUp(dial, { pointerId: 1 });

    expect(props.onCommit).toHaveBeenCalledTimes(1);
    expect(props.onCommit).toHaveBeenCalledWith("sparks_direction", 90, undefined);
  });

  it("край веера тянут — меняется sparks_spread, направление остаётся", () => {
    const { props, dial, part } = renderDial({ ...SPARKS, sparks_direction: 90 });

    fireEvent.pointerDown(part("spread"), { pointerId: 1, ...pointAtAngle(120) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(150) });
    fireEvent.pointerUp(dial, { pointerId: 1 });

    expect(props.onPreview).toHaveBeenLastCalledWith("sparks_spread", 60);
    expect(props.onCommit).toHaveBeenCalledWith("sparks_spread", 60, undefined);
    expect(screen.getByText("вправо, ±60°")).toBeTruthy();
  });

  it("с Ctrl направление и разброс идут шагом 15°", () => {
    const { props, dial, part } = renderDial();

    fireEvent.pointerDown(part("direction"), { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(dial, { pointerId: 1, ctrlKey: true, ...pointAtAngle(98) });
    fireEvent.pointerUp(dial, { pointerId: 1 });
    expect(props.onPreview).toHaveBeenLastCalledWith("sparks_direction", 105);

    fireEvent.pointerDown(part("spread"), { pointerId: 1, ...pointAtAngle(135) });
    fireEvent.pointerMove(dial, { pointerId: 1, ctrlKey: true, ...pointAtAngle(160) });
    fireEvent.pointerUp(dial, { pointerId: 1 });
    expect(props.onPreview).toHaveBeenLastCalledWith("sparks_spread", 60);
  });

  it("разброс не больше 180°", () => {
    const { props, dial, part } = renderDial();

    fireEvent.pointerDown(part("spread"), { pointerId: 1, ...pointAtAngle(30) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(180) });
    fireEvent.pointerUp(dial, { pointerId: 1 });

    expect(props.onCommit).toHaveBeenCalledWith("sparks_spread", 180, undefined);
  });

  it("за пределами ручек круг не тянется, при недоступной правке — тоже", () => {
    const { props, dial } = renderDial();
    fireEvent.pointerDown(dial, { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(90) });
    expect(props.onPreview).not.toHaveBeenCalled();
    cleanup();

    const disabled = renderPanel(SPARKS, { isEditable: false });
    const disabledDial = screen.getByRole("group", { name: "направление и разброс искр" });
    Object.defineProperty(disabledDial, "getBoundingClientRect", { value: () => ({ left: 0, top: 0, width: 100, height: 100 }) });
    fireEvent.pointerDown(disabledDial.querySelector("[data-part=direction]") as Element, { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(disabledDial, { pointerId: 1, ...pointAtAngle(90) });
    expect(disabled.props.onPreview).not.toHaveBeenCalled();
  });

  it("круг бледный, пока у объекта нет ни направления, ни разброса; с первого движения он уже показывает своё значение и не бледный", () => {
    const { dial, part } = renderDial();
    expect(dial.getAttribute("class")).toContain("particles-dial--default");

    fireEvent.pointerDown(part("direction"), { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(90) });
    expect(dial.getAttribute("class")).not.toContain("particles-dial--default");

    fireEvent.pointerUp(dial, { pointerId: 1 });
    expect(dial.getAttribute("class")).not.toContain("particles-dial--default");
  });

  it("ошибка движка стоит под кругом, запись не просят, сцене возвращено прежнее", () => {
    const onPreview = vi.fn((key: string, value: unknown) => (value === 200 ? undefined : key === "sparks_direction" && value === 90 ? "направление не принято" : undefined));
    const { props } = renderPanel(SPARKS, { onPreview });
    const dial = screen.getByRole("group", { name: "направление и разброс искр" });
    Object.defineProperty(dial, "getBoundingClientRect", { value: () => ({ left: 0, top: 0, width: 100, height: 100 }) });

    fireEvent.pointerDown(dial.querySelector("[data-part=direction]") as Element, { pointerId: 1, ...pointAtAngle(0) });
    fireEvent.pointerMove(dial, { pointerId: 1, ...pointAtAngle(90) });
    fireEvent.pointerUp(dial, { pointerId: 1 });

    expect(screen.getByRole("alert").textContent).toBe("направление не принято");
    expect(props.onCommit).not.toHaveBeenCalled();
    expect(onPreview).toHaveBeenLastCalledWith("sparks_direction", undefined);
  });
});
