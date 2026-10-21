// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type TerrainWater } from "./terrainFile";
import { WaterFields } from "./WaterFields";

const LAKE_AND_SEA: TerrainWater = { color: "#35607a", level: 0, lakes: [{ at: [40, 9], level: 4.3 }], clarity: 0.8 };
const LAKE_ONLY: TerrainWater = { color: "#35607a", lakes: [{ at: [40, 9], level: 4.3 }] };

function renderFields(water: TerrainWater | null): { onWaterChange: ReturnType<typeof vi.fn> } {
  const onWaterChange = vi.fn();
  render(<WaterFields water={water} onWaterChange={onWaterChange} />);
  return { onWaterChange };
}

function field(label: string): HTMLInputElement {
  return screen.getByLabelText(label) as HTMLInputElement;
}

function typeInto(input: HTMLInputElement, text: string): void {
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: text } });
  fireEvent.blur(input);
}

afterEach(cleanup);

describe("окошко «Вода»: поля", () => {
  it("подписи полных русских слов с единицей", () => {
    renderFields(LAKE_AND_SEA);

    expect(screen.getByLabelText("Море")).toBeTruthy();
    for (const label of ["Уровень моря", "Дно видно до глубины, клеток", "Длина ряби, клеток"]) expect(field(label)).toBeTruthy();
  });

  it("без воды всё, кроме галочки, неактивно", () => {
    renderFields(null);

    expect((screen.getByLabelText("Море") as HTMLInputElement).checked).toBe(false);
    for (const label of ["Уровень моря", "Дно видно до глубины, клеток", "Длина ряби, клеток"]) expect(field(label).disabled).toBe(true);
  });

  it("только озёра: галочка снята, уровень моря неактивен, цвет, прозрачность и рябь доступны", () => {
    renderFields(LAKE_ONLY);

    expect((screen.getByLabelText("Море") as HTMLInputElement).checked).toBe(false);
    expect(field("Уровень моря").disabled).toBe(true);
    expect(field("Дно видно до глубины, клеток").disabled).toBe(false);
    expect(field("Длина ряби, клеток").disabled).toBe(false);
  });

  it("галочка при озёрах снимает только море и озёра оставляет", () => {
    const { onWaterChange } = renderFields(LAKE_AND_SEA);

    fireEvent.click(screen.getByLabelText("Море"));

    expect(onWaterChange).toHaveBeenCalledWith({ color: "#35607a", lakes: LAKE_AND_SEA.lakes, clarity: 0.8 });
  });

  it("галочка без воды в файле даёт море на уровне −0,5 цветом по умолчанию", () => {
    const { onWaterChange } = renderFields(null);

    fireEvent.click(screen.getByLabelText("Море"));

    expect(onWaterChange).toHaveBeenCalledWith({ level: -0.5, color: "#3f7fd0" });
  });

  it("правка уровня моря сохраняет озёра и прозрачность", () => {
    const { onWaterChange } = renderFields(LAKE_AND_SEA);

    typeInto(field("Уровень моря"), "1,5");

    expect(onWaterChange).toHaveBeenCalledWith({ ...LAKE_AND_SEA, level: 1.5 });
  });

  it("поле прозрачности без значения пусто и показывает 1; набранное пишется до сотых", () => {
    const { onWaterChange } = renderFields(LAKE_ONLY);

    expect(field("Дно видно до глубины, клеток").value).toBe("");
    expect(field("Дно видно до глубины, клеток").placeholder).toBe("1");
    typeInto(field("Дно видно до глубины, клеток"), "2.345");

    expect(onWaterChange).toHaveBeenCalledWith({ ...LAKE_ONLY, clarity: 2.35 });
  });

  it("стёртое поле возвращает значение по умолчанию: ключа в воде нет", () => {
    const { onWaterChange } = renderFields(LAKE_AND_SEA);

    expect(field("Дно видно до глубины, клеток").value).toBe("0.8");
    typeInto(field("Дно видно до глубины, клеток"), "");

    expect(onWaterChange).toHaveBeenCalledWith({ color: "#35607a", level: 0, lakes: LAKE_AND_SEA.lakes });
  });

  it("пустое поле, которое и так пусто, и не число воду не меняют; ноль и минус прижимаются к сотой", () => {
    const { onWaterChange } = renderFields(LAKE_ONLY);

    typeInto(field("Длина ряби, клеток"), "");
    typeInto(field("Длина ряби, клеток"), "abc");
    expect(onWaterChange).not.toHaveBeenCalled();

    typeInto(field("Длина ряби, клеток"), "-3");
    expect(onWaterChange).toHaveBeenCalledWith({ ...LAKE_ONLY, ripple: 0.01 });
  });
});
