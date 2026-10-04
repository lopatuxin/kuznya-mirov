import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ImprintToolFields, nonZeroHundredths, positiveHundredths } from "./ImprintToolFields";

const NOOP = (): void => {};

describe("поле «Высота» отпечатка", () => {
  it("принимает число любого знака до сотых: −3 принимается", () => {
    const normalize = nonZeroHundredths(10);
    expect(normalize(-3)).toBe(-3);
    expect(normalize(7.456)).toBe(7.46);
    expect(normalize(-0.126)).toBe(-0.13);
  });

  it("ноль, в том числе после округления, возвращает прежнее значение", () => {
    const normalize = nonZeroHundredths(-3);
    expect(normalize(0)).toBe(-3);
    expect(normalize(-0)).toBe(-3);
    expect(normalize(0.004)).toBe(-3);
    expect(normalize(-0.004)).toBe(-3);
  });

  it("«Ширина» по-прежнему больше нуля: отрицательная и ноль возвращают прежнее значение", () => {
    const normalize = positiveHundredths(30);
    expect(normalize(12.345)).toBe(12.35);
    expect(normalize(-5)).toBe(30);
    expect(normalize(0)).toBe(30);
  });

  it("поля «Штамп», «Ширина», «Высота» под подписью «Отпечаток»; у высоты подсказка «Меньше нуля — вдавливает»", () => {
    const html = renderToStaticMarkup(
      <ImprintToolFields stampNames={["hill", "ravine"]} stamp="ravine" width={30} height={-4} onStampChange={NOOP} onWidthChange={NOOP} onHeightChange={NOOP} />,
    );

    expect(html).toContain('role="group" aria-label="Отпечаток"');
    expect(html).toContain(">Отпечаток</span>");
    expect(html).toContain('<option value="ravine" selected="">ravine</option>');
    expect(html).toMatch(/Ширина<input[^>]*value="30"/);
    expect(html).toMatch(/<label[^>]*title="Меньше нуля — вдавливает"[^>]*>Высота<input[^>]*value="-4"/);
    expect(html).not.toMatch(/title="[^"]*"[^>]*>Ширина/);
  });
});
