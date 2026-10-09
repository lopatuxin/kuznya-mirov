// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CloudsGroup } from "./CloudsGroup";
import type { ProjectImageTile } from "./projectImages";

type GroupProps = Parameters<typeof CloudsGroup>[0];

function pictureTile(name: string): ProjectImageTile {
  return { description: { name, frames: null, columns: null, size: null, smooth: false }, image: null };
}

const PICTURES = { tiles: ["cloud_a", "cloud_b", "cloud_c", "hero"].map(pictureTile), cloudNames: ["cloud_a", "cloud_b", "cloud_c"] };
const SKY = { position: [0, 0], size: [20, 10], repeat_x: true };

function renderGroup(properties: GroupProps["properties"], overrides: Partial<GroupProps> = {}): { props: GroupProps } {
  const props: GroupProps = {
    properties,
    isEditable: true,
    images: PICTURES,
    handlers: {
      onPreview: vi.fn(() => undefined),
      onCommit: vi.fn(),
      onRemove: vi.fn(),
      onGestureActiveChange: vi.fn(),
    },
    ...overrides,
  };
  render(<CloudsGroup {...props} />);
  return { props };
}

function group(): HTMLElement {
  return screen.getByRole("region", { name: "Облака" });
}

afterEach(cleanup);

describe("CloudsGroup: когда она есть", () => {
  it("у объекта без position, size или включённого repeat_x группы нет", () => {
    for (const properties of [{ position: [0, 0], size: [20, 10] }, { position: [0, 0], size: [20, 10], repeat_x: false }, { size: [20, 10], repeat_x: true }, { position: [0, 0], repeat_x: true }, { clouds: 0.3 }]) {
      renderGroup(properties);
      expect(screen.queryByRole("region", { name: "Облака" })).toBeNull();
      cleanup();
    }
  });

  it("небо без clouds — одна кнопка «Добавить облака», она пишет clouds 0,3 одной правкой без прежнего значения", () => {
    const { props } = renderGroup(SKY);

    expect(within(group()).getAllByRole("button").map((button) => button.textContent)).toEqual(["Добавить облака"]);
    expect(screen.queryByRole("slider")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Добавить облака" }));

    expect(props.handlers.onCommit).toHaveBeenCalledTimes(1);
    expect(props.handlers.onCommit).toHaveBeenCalledWith("clouds", 0.3, undefined);
  });

  it("правка недоступна — «Добавить облака» неактивна", () => {
    renderGroup(SKY, { isEditable: false });

    expect((screen.getByRole("button", { name: "Добавить облака" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("CloudsGroup: настройки неба с облаками", () => {
  it("ползунок «сколько облаков» от 0 до 1 шагом 0,05 с русскими концами и без английских ключей", () => {
    renderGroup({ ...SKY, clouds: 0.3 });
    const slider = screen.getByRole("slider", { name: "сколько облаков" }) as HTMLInputElement;

    expect([slider.min, slider.max, slider.step, slider.value]).toEqual(["0", "1", "0.05", "0.3"]);
    for (const text of ["сколько облаков", "картинки облаков", "редкие", "небо в облаках"]) expect(within(group()).getByText(text)).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/clouds|cloud_images/);
  });

  it("ползунок: значение сцене на каждое движение, запись при отпускании, ошибка движка — с подписью «сколько облаков»", () => {
    const onPreview = vi.fn((key: string, value: unknown) => (value === 0.9 ? `${key}: нужно от 0 до 1 включительно, получено ${value}` : undefined));
    const { props } = renderGroup({ ...SKY, clouds: 0.3 }, { handlers: { onPreview, onCommit: vi.fn(), onRemove: vi.fn(), onGestureActiveChange: vi.fn() } });
    const slider = screen.getByRole("slider", { name: "сколько облаков" });

    fireEvent.input(slider, { target: { value: "0.6" } });
    expect(onPreview).toHaveBeenLastCalledWith("clouds", 0.6);
    expect(props.handlers.onCommit).not.toHaveBeenCalled();
    fireEvent.change(slider);
    expect(props.handlers.onCommit).toHaveBeenCalledTimes(1);
    expect(props.handlers.onCommit).toHaveBeenCalledWith("clouds", 0.6, 0.3);

    fireEvent.input(slider, { target: { value: "0.9" } });
    expect(screen.getByRole("alert").textContent).toBe("сколько облаков: нужно от 0 до 1 включительно, получено 0.9");
  });

  it("картинок нет — под рядом подсказка, ряда нет", () => {
    renderGroup({ ...SKY, clouds: 0.3 });

    expect(within(group()).getByText("Выберите картинки облаков — без них облаков нет")).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Выбранные картинки облаков" })).toBeNull();
  });

  it("«+ картинка» показывает только годные и ещё не выбранные; щелчок добавляет в конец одной правкой списка", () => {
    const { props } = renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_b"] });
    expect(screen.getByRole("list", { name: "Выбранные картинки облаков" }).textContent).toBe("cloud_b");

    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    const choices = screen.getByRole("list", { name: "Картинки, годные облакам" });
    expect(within(choices).getAllByRole("button").map((button) => button.textContent)).toEqual(["cloud_a", "cloud_c"]);

    fireEvent.click(within(choices).getByRole("button", { name: "cloud_c" }));

    expect(props.handlers.onCommit).toHaveBeenCalledTimes(1);
    expect(props.handlers.onCommit).toHaveBeenCalledWith("cloud_images", ["cloud_b", "cloud_c"], ["cloud_b"]);
    expect(within(screen.getByRole("list", { name: "Выбранные картинки облаков" })).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["cloud_b", "cloud_c"]);
  });

  it("первая картинка пишет список там, где его не было: прежнего значения нет", () => {
    const { props } = renderGroup({ ...SKY, clouds: 0.3 });

    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    fireEvent.click(screen.getByRole("button", { name: "cloud_a" }));

    expect(props.handlers.onCommit).toHaveBeenCalledWith("cloud_images", ["cloud_a"], undefined);
  });

  it("крестик убирает одну картинку из списка одной правкой", () => {
    const { props } = renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_a", "cloud_b"] });

    fireEvent.click(screen.getByRole("button", { name: "Убрать картинку cloud_a" }));

    expect(props.handlers.onCommit).toHaveBeenCalledTimes(1);
    expect(props.handlers.onCommit).toHaveBeenCalledWith("cloud_images", ["cloud_b"], ["cloud_a", "cloud_b"]);
  });

  it("список, который движок не принял, не пишется, а отказ показан под рядом с русской подписью", () => {
    const onPreview = vi.fn((key: string) => (key === "cloud_images" ? "cloud_images: картинка cloud_b — видео" : undefined));
    const { props } = renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_a"] }, { handlers: { onPreview, onCommit: vi.fn(), onRemove: vi.fn(), onGestureActiveChange: vi.fn() } });

    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    fireEvent.click(within(screen.getByRole("list", { name: "Картинки, годные облакам" })).getByText("cloud_b"));

    expect(onPreview).toHaveBeenCalledWith("cloud_images", ["cloud_a", "cloud_b"]);
    expect(props.handlers.onCommit).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toBe("картинки облаков: картинка cloud_b — видео");
    expect(screen.getByRole("list", { name: "Выбранные картинки облаков" }).textContent).toBe("cloud_a");
  });

  it("список закрывается щелчком мимо и Esc, щелчок внутри списка его не закрывает", () => {
    renderGroup({ ...SKY, clouds: 0.3 });
    const openList = (): void => {
      fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    };
    const isListShown = (): boolean => screen.queryByRole("list", { name: "Картинки, годные облакам" }) !== null;

    openList();
    fireEvent.pointerDown(document.body);
    expect(isListShown()).toBe(false);

    openList();
    fireEvent.keyDown(document.body, { code: "Escape" });
    expect(isListShown()).toBe(false);

    openList();
    fireEvent.pointerDown(screen.getByRole("list", { name: "Картинки, годные облакам" }));
    expect(isListShown()).toBe(true);
  });

  it("годных картинок не осталось — в списке об этом сказано", () => {
    renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_a", "cloud_b", "cloud_c"] });

    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));

    expect(screen.getByText("Других подходящих картинок в игре нет")).toBeTruthy();
  });

  it("«Убрать» снимает clouds и cloud_images одной правкой; без cloud_images — только clouds", () => {
    const { props } = renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_a"] });

    fireEvent.click(within(group()).getByRole("button", { name: "Убрать" }));
    expect(props.handlers.onRemove).toHaveBeenCalledTimes(1);
    expect(props.handlers.onRemove).toHaveBeenCalledWith(["clouds", "cloud_images"]);
    cleanup();

    const { props: bare } = renderGroup({ ...SKY, clouds: 0.3 });
    fireEvent.click(within(group()).getByRole("button", { name: "Убрать" }));
    expect(bare.handlers.onRemove).toHaveBeenCalledWith(["clouds"]);
  });

  it("правка недоступна — ползунок, «+ картинка», крестик и «Убрать» неактивны", () => {
    renderGroup({ ...SKY, clouds: 0.3, cloud_images: ["cloud_a"] }, { isEditable: false });

    expect((screen.getByRole("slider", { name: "сколько облаков" }) as HTMLInputElement).disabled).toBe(true);
    for (const name of ["+ картинка", "Убрать картинку cloud_a", "Убрать"]) expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("открытый список картинок пропадает, когда правка стала недоступной, — выбрать из него в повторе нельзя", () => {
    const handlers = { onPreview: vi.fn(() => undefined), onCommit: vi.fn(), onRemove: vi.fn(), onGestureActiveChange: vi.fn() };
    const properties = { ...SKY, clouds: 0.3 };
    const { rerender } = render(<CloudsGroup properties={properties} isEditable images={PICTURES} handlers={handlers} />);
    fireEvent.click(screen.getByRole("button", { name: "+ картинка" }));
    expect(screen.queryByRole("list", { name: "Картинки, годные облакам" })).not.toBeNull();

    rerender(<CloudsGroup properties={properties} isEditable={false} images={PICTURES} handlers={handlers} />);

    expect(screen.queryByRole("list", { name: "Картинки, годные облакам" })).toBeNull();
  });

  it("жест, не дошедший до отпускания, при уходе группы отпускает перезагрузку файлов", () => {
    const { props } = renderGroup({ ...SKY, clouds: 0.3 });

    fireEvent.input(screen.getByRole("slider", { name: "сколько облаков" }), { target: { value: "0.6" } });
    expect(props.handlers.onGestureActiveChange).toHaveBeenLastCalledWith(true);
    cleanup();

    expect(props.handlers.onGestureActiveChange).toHaveBeenLastCalledWith(false);
  });
});
