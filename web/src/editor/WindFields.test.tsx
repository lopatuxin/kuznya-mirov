// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WindFields } from "./WindFields";
import { WindMenu } from "./WindMenu";

const NOOP = (): undefined => undefined;

describe("WindFields", () => {
  it("показывает ветер по x и по y", () => {
    const markup = renderToStaticMarkup(<WindFields wind={[1.5, -0.25]} onWindChange={NOOP} />);
    expect(markup).toContain("По x");
    expect(markup).toContain("По y");
    expect(markup).toContain('value="1.5"');
    expect(markup).toContain('value="-0.25"');
  });
});

describe("WindFields: ввод и ошибка движка", () => {
  afterEach(cleanup);

  function typeAndLeave(label: "По x" | "По y", text: string): void {
    const input = screen.getByLabelText(label);
    act(() => input.focus());
    fireEvent.change(input, { target: { value: text } });
    act(() => input.blur());
  }

  it("принятое значение уходит обработчику вместе с другой осью", () => {
    const onWindChange = vi.fn(() => undefined);
    render(<WindFields wind={[1.5, 0.25]} onWindChange={onWindChange} />);

    typeAndLeave("По x", "-2");

    expect(onWindChange.mock.calls).toEqual([[[-2, 0.25]]]);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ошибка движка видна под полями", () => {
    render(<WindFields wind={[1.5, 0]} onWindChange={() => "ветер не пара чисел"} />);

    typeAndLeave("По y", "4");

    expect(screen.getByRole("alert").textContent).toBe("ветер не пара чисел");
  });

  it("ветер не менялся — ошибка остаётся, поменялся снаружи — пропадает", () => {
    const { rerender } = render(<WindFields wind={[1.5, 0]} onWindChange={() => "ветер не пара чисел"} />);
    typeAndLeave("По x", "4");

    rerender(<WindFields wind={[1.5, 0]} onWindChange={() => "ветер не пара чисел"} />);
    expect(screen.getByRole("alert")).toBeTruthy();

    rerender(<WindFields wind={[-1, 0]} onWindChange={() => "ветер не пара чисел"} />);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("следующий принятый ввод снимает прежнюю ошибку", () => {
    const onWindChange = vi.fn<(wind: readonly [number, number]) => string | undefined>().mockReturnValueOnce("ветер не пара чисел").mockReturnValue(undefined);
    render(<WindFields wind={[1.5, 0]} onWindChange={onWindChange} />);

    typeAndLeave("По x", "4");
    expect(screen.getByRole("alert")).toBeTruthy();
    typeAndLeave("По y", "2");

    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("WindMenu", () => {
  it("кнопка «Ветер» активна, окошко закрыто, пока по ней не щёлкнули", () => {
    const markup = renderToStaticMarkup(<WindMenu wind={[1.5, 0]} isDisabled={false} onWindChange={NOOP} />);
    expect(markup).toContain("Ветер");
    expect(markup).not.toContain("disabled");
    expect(markup).not.toContain("По x");
  });

  it("при ошибках проекта и в повторе кнопка неактивна", () => {
    const markup = renderToStaticMarkup(<WindMenu wind={[0, 0]} isDisabled onWindChange={NOOP} />);
    expect(markup).toContain("disabled");
  });
});
