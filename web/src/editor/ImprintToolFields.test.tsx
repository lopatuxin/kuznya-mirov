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

  it("штампы — карточками с названием, выбранная отмечена; «Ширина», «Высота» с подсказкой «Меньше нуля — вдавливает»", () => {
    const html = renderToStaticMarkup(
      <ImprintToolFields stampNames={["hill", "ravine"]} stamp="ravine" previews={new Map()} width={30} height={-4} onStampChange={NOOP} onWidthChange={NOOP} onHeightChange={NOOP} />,
    );

    expect(html).toContain('role="group" aria-label="Отпечаток"');
    const cards = Array.from(html.matchAll(/role="radio" aria-checked="(true|false)"[^>]*title="([^"]*)"/g)).map((match) => [match[2], match[1]]);
    expect(cards).toEqual([
      ["hill", "false"],
      ["ravine", "true"],
    ]);
    expect(html).not.toContain("<img");
    expect(html).toMatch(/Ширина<input[^>]*value="30"/);
    expect(html).toMatch(/<label[^>]*title="Меньше нуля — вдавливает"[^>]*>Высота<input[^>]*value="-4"/);
    expect(html).not.toMatch(/title="[^"]*"[^>]*>Ширина/);
  });

  it("картинка штампа — холмом, а при высоте меньше нуля — впадиной", () => {
    const previews = new Map([["hill", { raised: "raised.png", lowered: "lowered.png" }]]);
    const render = (height: number): string =>
      renderToStaticMarkup(<ImprintToolFields stampNames={["hill"]} stamp="hill" previews={previews} width={30} height={height} onStampChange={NOOP} onWidthChange={NOOP} onHeightChange={NOOP} />);

    expect(render(4)).toContain('src="raised.png"');
    expect(render(-4)).toContain('src="lowered.png"');
  });
});
