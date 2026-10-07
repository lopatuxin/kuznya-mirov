// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PARTICLES_DRAG_TYPE } from "./imageDrag";
import { readParticleKinds, type ParticleKind, type ParticleTable } from "./particlesFile";
import { ParticlesPanel } from "./ParticlesPanel";
import { tabParticleKinds, type TabParticleKind } from "./particlePresets";
import type { ProjectImageTile } from "./projectImages";

const FILE = `{
  "дым": { "image": "puff", "rate": 6, "lifetime": [4, 6], "size": 0.6, "opacity": [0, 0.7, 0] },
  "искры": { "image": "spark", "rate": 20, "lifetime": 1, "size": 0.2 }
}`;

/** Свои виды файла — не готовые: их можно удалить и переименовать. */
function own(kinds: readonly ParticleKind[]): TabParticleKind[] {
  return kinds.map((kind) => ({ ...kind, isBuiltIn: false, isInFile: true }));
}

function tile(name: string): ProjectImageTile {
  return { description: { name, frames: null, columns: null, size: null, smooth: false }, image: null };
}

type PanelProps = Parameters<typeof ParticlesPanel>[0];

function renderPanel(overrides: Partial<PanelProps> = {}): { props: PanelProps; rerender: (ui: React.JSX.Element) => void } {
  const props: PanelProps = {
    kinds: own(readParticleKinds(FILE)),
    imageTiles: [tile("puff"), tile("spark")],
    selectedName: null,
    invalidNames: new Set(),
    loadFieldErrors: new Map(),
    isFieldsDisabled: false,
    kindActionsBlockedReason: null,
    isDragEnabled: true,
    onSelect: vi.fn(),
    onPreview: vi.fn(() => undefined),
    onCommitValue: vi.fn(() => undefined),
    onCopy: vi.fn(),
    onDelete: vi.fn(),
    onRename: vi.fn(),
    ...overrides,
  };
  const { rerender } = render(<ParticlesPanel {...props} />);
  return { props, rerender };
}

function field(label: string): HTMLInputElement {
  return screen.getByLabelText(label);
}

function type(input: HTMLElement, text: string): void {
  act(() => input.focus());
  fireEvent.change(input, { target: { value: text } });
}

function leave(input: HTMLElement): void {
  act(() => input.blur());
}

describe("ParticlesPanel: готовые виды", () => {
  afterEach(cleanup);

  it("в проекте без видов во вкладке уже есть дым, искры и листья — каждый со своим рисунком, без картинок", () => {
    renderPanel({ kinds: tabParticleKinds([]) });

    const cards = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(cards.map((card) => card.textContent)).toEqual(["дым", "искры", "листья"]);
    expect(cards.every((card) => card.querySelector("svg.particles-card__shape") !== null)).toBe(true);
    expect(screen.queryByLabelText("картинка")).toBeNull();
    expect(field("густота").value).toBe("12");
  });

  it("готовый вид не удаляется и не переименовывается, а копируется", () => {
    const { props } = renderPanel({ kinds: tabParticleKinds([]) });

    expect((screen.getByRole("button", { name: "Удалить" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "дым" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Копировать" }));
    expect(props.onCopy).toHaveBeenCalledWith("дым");
  });

  it("свой вид из файла идёт после готовых", () => {
    renderPanel({ kinds: tabParticleKinds(readParticleKinds('{ "туман": { "rate": 2, "lifetime": 3, "size": 1 } }')) });

    expect(screen.getAllByRole("option").map((card) => card.textContent)).toEqual(["дым", "искры", "листья", "туман"]);
  });
});

describe("ParticlesPanel: карточки", () => {
  afterEach(cleanup);

  it("карточки в порядке видов, при открытии выбрана первая", () => {
    renderPanel();

    const cards = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(cards.map((card) => card.textContent)).toEqual(["дым", "искры"]);
    expect(cards.map((card) => card.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    expect(screen.getByText("дым", { selector: ".particles-name" })).toBeTruthy();
  });

  it("выбранный вид — названный; щелчок по карточке выбирает", () => {
    const { props } = renderPanel({ selectedName: "искры" });

    expect(field("густота").value).toBe("20");
    fireEvent.click(screen.getAllByRole("option")[0] as HTMLElement);
    expect(props.onSelect).toHaveBeenCalledWith("дым");
  });

  it("карточка перетаскивается на сцену с именем вида; в повторе — нет", () => {
    renderPanel();
    const setData = vi.fn();
    fireEvent.dragStart(screen.getAllByRole("option")[1] as HTMLElement, { dataTransfer: { setData, effectAllowed: "" } });
    expect(setData).toHaveBeenCalledWith(PARTICLES_DRAG_TYPE, "искры");
    cleanup();

    renderPanel({ isDragEnabled: false });
    expect(screen.getAllByRole("option")[0]?.getAttribute("draggable")).toBe("false");
  });

  it("вид без картинки и рисунка показан на карточке мягкой точкой", () => {
    renderPanel({ kinds: own(readParticleKinds('{ "точки": { "rate": 8, "lifetime": 2, "size": 0.4 } }')) });

    expect(screen.getAllByRole("option")[0]?.querySelector(".particles-card__dot")).not.toBeNull();
  });

  it("вид с ошибкой проверки помечен на карточке, у неизвестной картинки — пустой рисунок", () => {
    renderPanel({ invalidNames: new Set(["искры"]), imageTiles: [tile("puff")] });

    const cards = screen.getAllByRole("option");
    expect(cards[0]?.className).not.toContain("particles-card--invalid");
    expect(cards[1]?.className).toContain("particles-card--invalid");
    expect(cards[1]?.querySelector(".images-panel__thumb--empty")).not.toBeNull();
  });
});

describe("ParticlesPanel: кнопки", () => {
  afterEach(cleanup);

  it("«Копировать» и «Удалить» зовут действие с выбранным видом", () => {
    const { props } = renderPanel({ selectedName: "искры" });

    fireEvent.click(screen.getByRole("button", { name: "Копировать" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));

    expect(props.onCopy).toHaveBeenCalledWith("искры");
    expect(props.onDelete).toHaveBeenCalledWith("искры");
  });

  it("в партии «Копировать», «Удалить» и правка имени неактивны, поля — нет", () => {
    renderPanel({ kindActionsBlockedReason: "после «Стопа»" });

    for (const name of ["Копировать", "Удалить"]) expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "дым" }) as HTMLButtonElement).disabled).toBe(true);
    expect(field("густота").disabled).toBe(false);
  });

  it("в повторе неактивно всё", () => {
    renderPanel({ kindActionsBlockedReason: "в повторе мир не правится", isFieldsDisabled: true, isDragEnabled: false });

    expect(field("густота").disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Копировать" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ParticlesPanel: поля", () => {
  afterEach(cleanup);

  it("три группы с подписями и подсказкой — ключом файла", () => {
    renderPanel();

    expect(screen.getAllByRole("region").map((group) => group.getAttribute("aria-label"))).toEqual(["Вылет", "Вид", "Полёт"]);
    expect(screen.getByText("густота").closest(".particles-field")?.getAttribute("title")).toMatch(/^rate — /);
  });

  it("значения из файла; ключа нет — значение по умолчанию бледно", () => {
    renderPanel();

    expect(field("густота").value).toBe("6");
    expect(field("густота").className).not.toContain("particles-input--default");
    expect(field("тяжесть").value).toBe("0");
    expect(field("тяжесть").className).toContain("particles-input--default");
    expect(field("ветер, доля").value).toBe("1");
    expect(field("ветер, доля").className).toContain("particles-input--default");
  });

  it("пара — одно поле «от – до», число — само, с запятой", () => {
    renderPanel();

    expect(field("живёт, с").value).toBe("4 – 6");
    expect(field("размер").value).toBe("0,6");
  });

  it("ошибка на весь вид — «и image, и shape» — видна под именем вида", () => {
    renderPanel({ loadFieldErrors: new Map([["дым", { "": "у вида и image, и shape — оставьте одно" }]]) });

    expect(screen.getByRole("alert").textContent).toBe("у вида и image, и shape — оставьте одно");
  });

  it("ошибка картинки, записанной в файл руками, видна под именем вида", () => {
    renderPanel({ loadFieldErrors: new Map([["дым", { image: "картинки «puff» нет" }]]) });

    expect(screen.getByRole("alert").textContent).toBe("картинки «puff» нет");
  });
});

describe("ParticlesPanel: ввод в поле", () => {
  afterEach(cleanup);

  it("каждое набранное значение сразу уходит движку, файл не пишется", () => {
    const { props } = renderPanel();

    type(field("тяжесть"), "1");
    type(field("тяжесть"), "1,5");

    const previews = vi.mocked(props.onPreview).mock.calls.map(([table]) => (table as ParticleTable).дым?.gravity);
    expect(previews).toEqual([1, 1.5]);
    expect(props.onCommitValue).not.toHaveBeenCalled();
  });

  it("не число не уходит движку", () => {
    const { props } = renderPanel();

    type(field("тяжесть"), "тяж");

    expect(props.onPreview).not.toHaveBeenCalled();
  });

  it("Enter принимает значение действием вместе с таблицей видов", () => {
    const { props } = renderPanel();

    type(field("рост, раз"), "3");
    fireEvent.keyDown(field("рост, раз"), { key: "Enter" });
    leave(field("рост, раз"));

    expect(props.onCommitValue).toHaveBeenCalledOnce();
    const [name, key, value, table] = vi.mocked(props.onCommitValue).mock.calls[0] as [string, string, unknown, ParticleTable];
    expect([name, key, value]).toEqual(["дым", "grow", 3]);
    expect(table.дым?.grow).toBe(3);
    expect(table.искры?.grow).toBeUndefined();
  });

  it("уход из поля принимает значение", () => {
    const { props } = renderPanel();

    type(field("густота"), "9");
    leave(field("густота"));

    expect(vi.mocked(props.onCommitValue).mock.calls[0]?.slice(0, 3)).toEqual(["дым", "rate", 9]);
  });

  it("Esc возвращает значение до правки: движку уходят виды как в файле, действия нет", () => {
    const { props } = renderPanel();

    type(field("густота"), "9");
    fireEvent.keyDown(field("густота"), { key: "Escape" });
    leave(field("густота"));

    expect(props.onCommitValue).not.toHaveBeenCalled();
    expect(vi.mocked(props.onPreview).mock.calls.at(-1)?.[0]).toEqual({
      дым: readParticleKinds(FILE)[0]?.fields,
      искры: readParticleKinds(FILE)[1]?.fields,
    });
    expect(field("густота").value).toBe("6");
  });

  it("значение не изменилось — действия нет", () => {
    const { props } = renderPanel();

    type(field("густота"), "6");
    leave(field("густота"));

    expect(props.onCommitValue).not.toHaveBeenCalled();
  });

  it("пустое поле убирает ключ", () => {
    const { props } = renderPanel();

    type(field("размер"), "");
    leave(field("размер"));

    expect(vi.mocked(props.onCommitValue).mock.calls[0]?.slice(0, 3)).toEqual(["дым", "size", undefined]);
  });

  it("пара равных значений пишется одним числом, разных — парой", () => {
    const { props } = renderPanel();

    type(field("живёт, с"), "6 – 6");
    leave(field("живёт, с"));
    type(field("живёт, с"), "6-9");
    leave(field("живёт, с"));

    expect(vi.mocked(props.onCommitValue).mock.calls.map((call) => call[2])).toEqual([6, [6, 9]]);
  });

  it("принятое значение стоит в поле, пока файл ещё не показал его", () => {
    renderPanel();

    type(field("живёт, с"), "5 – 9");
    fireEvent.keyDown(field("живёт, с"), { key: "Enter" });
    leave(field("живёт, с"));

    expect(field("живёт, с").value).toBe("5 – 9");
  });

  it("когда значение дошло из файла, показано оно, а не набранный текст", () => {
    const { props, rerender } = renderPanel();
    type(field("живёт, с"), "5 – 9");
    fireEvent.keyDown(field("живёт, с"), { key: "Enter" });

    rerender(<ParticlesPanel {...props} kinds={own(readParticleKinds(FILE.replace("[4, 6]", "[5.5, 6]")))} />);

    expect(field("живёт, с").value).toBe("5,5 – 6");
  });

  it("не число при уходе из поля не принимается, виды возвращаются как в файле", () => {
    const { props } = renderPanel();

    type(field("густота"), "много");
    leave(field("густота"));

    expect(props.onCommitValue).not.toHaveBeenCalled();
    expect(props.onPreview).toHaveBeenCalledOnce();
  });

  it("ошибка движка стоит у поля, которое её дало, и пропадает, когда ввод принят", () => {
    const onPreview = vi.fn<(table: ParticleTable) => string | undefined>().mockReturnValueOnce("дым → rate: должно быть больше нуля").mockReturnValue(undefined);
    renderPanel({ onPreview });

    type(field("густота"), "0");
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("дым → rate: должно быть больше нуля");
    expect(alert.closest(".particles-field")?.querySelector("input")).toBe(field("густота"));

    type(field("густота"), "3");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("отказ принять значение (партия): ошибка у своего поля, в поле отвергнутое число, правка другого поля ошибку не снимает, Esc возвращает действующее", () => {
    const onCommitValue = vi.fn<PanelProps["onCommitValue"]>().mockReturnValueOnce("в таблице нет вида").mockReturnValue(undefined);
    const { props } = renderPanel({ onCommitValue });

    type(field("густота"), "9");
    leave(field("густота"));

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("в таблице нет вида");
    expect(alert.closest(".particles-field")?.querySelector("input")).toBe(field("густота"));
    expect(field("густота").value).toBe("9");

    type(field("тяжесть"), "2");
    leave(field("тяжесть"));

    expect(onCommitValue).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("alert").textContent).toBe("в таблице нет вида");
    expect(field("густота").value).toBe("9");

    fireEvent.keyDown(field("густота"), { key: "Escape" });

    expect(screen.queryByRole("alert")).toBeNull();
    expect(field("густота").value).toBe("6");
    expect(vi.mocked(props.onPreview).mock.calls.at(-1)?.[0]).toEqual({
      дым: readParticleKinds(FILE)[0]?.fields,
      искры: readParticleKinds(FILE)[1]?.fields,
    });
  });

  it("отказ у поля пары и у ряда точек тоже виден, пока действующее значение прежнее", () => {
    renderPanel({ onCommitValue: () => "в таблице нет вида" });

    type(field("видимость"), "0 · 1");
    leave(field("видимость"));

    expect(screen.getByRole("alert").closest(".particles-field")?.querySelector("input")).toBe(field("видимость"));
    expect(field("видимость").value).toBe("0 · 1");
  });

  it("отказ принять значение стоит, пока поле не набрали снова: новый ввод снимает прежнюю ошибку", () => {
    renderPanel({ onCommitValue: () => "в таблице нет вида" });
    type(field("густота"), "9");
    leave(field("густота"));

    type(field("густота"), "8");

    expect(screen.queryByRole("alert")).toBeNull();
    expect(field("густота").value).toBe("8");
  });

  it("ошибка проверки загрузкой стоит у своего поля и пропадает, когда файл её больше не даёт", () => {
    const loadFieldErrors = new Map([["дым", { rate: "rate: должно быть больше нуля" }]]);
    const { props, rerender } = renderPanel({ loadFieldErrors });

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("rate: должно быть больше нуля");
    expect(alert.closest(".particles-field")?.querySelector("input")).toBe(field("густота"));

    rerender(<ParticlesPanel {...props} loadFieldErrors={new Map()} />);

    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("ParticlesPanel: точки видимости", () => {
  afterEach(cleanup);

  it("ряд точек — одно поле через точку посередине", () => {
    renderPanel();

    expect(field("видимость").value).toBe("0 · 0,7 · 0");
  });

  it("правка точек пишет ряд целиком; через пробел тоже можно", () => {
    const { props } = renderPanel();

    type(field("видимость"), "0 · 0,9 · 0");
    leave(field("видимость"));
    type(field("видимость"), "0 1 0,2 0");
    leave(field("видимость"));

    expect(vi.mocked(props.onCommitValue).mock.calls.map((call) => call[2])).toEqual([
      [0, 0.9, 0],
      [0, 1, 0.2, 0],
    ]);
  });

  it("одна точка пишется числом", () => {
    const { props } = renderPanel();

    type(field("видимость"), "0,5");
    leave(field("видимость"));

    expect(vi.mocked(props.onCommitValue).mock.calls[0]?.[2]).toBe(0.5);
  });

  it("ключа нет — точка по умолчанию бледно", () => {
    renderPanel({ selectedName: "искры" });

    expect(field("видимость").value).toBe("1");
    expect(field("видимость").className).toContain("particles-input--default");
  });
});

describe("ParticlesPanel: имя вида", () => {
  afterEach(cleanup);

  function startRename(): HTMLInputElement {
    fireEvent.click(screen.getByRole("button", { name: "дым" }));
    return screen.getByLabelText("Имя вида");
  }

  it("щелчок по имени, Enter — вид переименован", () => {
    const { props } = renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "туман" } });
    fireEvent.keyDown(input, { key: "Enter" });
    act(() => input.blur());

    expect(props.onRename).toHaveBeenCalledWith("дым", "туман");
    expect(screen.queryByLabelText("Имя вида")).toBeNull();
  });

  it("пустое имя и имя другого вида — текст ошибки, ничего не пишется", () => {
    const { props } = renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "" } });
    expect(screen.getByRole("alert").textContent).toBe("Имя вида не может быть пустым");
    fireEvent.change(input, { target: { value: "искры" } });
    expect(screen.getByRole("alert").textContent).toBe("Вид «искры» уже есть");
    act(() => input.blur());

    expect(props.onRename).not.toHaveBeenCalled();
  });

  it("Enter с именем другого вида или пустым правку не закрывает: текст ошибки виден, ничего не пишется", () => {
    const { props } = renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "искры" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByLabelText("Имя вида")).toBe(input);
    expect(screen.getByRole("alert").textContent).toBe("Вид «искры» уже есть");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByLabelText("Имя вида")).toBe(input);
    expect(screen.getByRole("alert").textContent).toBe("Имя вида не может быть пустым");
    expect(props.onRename).not.toHaveBeenCalled();
  });

  it("после Enter с ошибкой уход фокуса закрывает правку, прежнее имя на месте", () => {
    renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "искры" } });
    fireEvent.keyDown(input, { key: "Enter" });
    act(() => input.blur());

    expect(screen.queryByLabelText("Имя вида")).toBeNull();
    expect(screen.getByRole("button", { name: "дым" })).toBeTruthy();

    startRename();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("принятое имя ошибки не оставляет", () => {
    renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "туман" } });
    act(() => input.blur());

    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("Esc отменяет правку имени", () => {
    const { props } = renderPanel();

    const input = startRename();
    fireEvent.change(input, { target: { value: "туман" } });
    fireEvent.keyDown(input, { key: "Escape" });

    expect(screen.queryByLabelText("Имя вида")).toBeNull();
    expect(props.onRename).not.toHaveBeenCalled();
  });
});
