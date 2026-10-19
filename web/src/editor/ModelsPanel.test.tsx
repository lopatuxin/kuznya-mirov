import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ModelsPanel } from "./ModelsPanel";
import { ProblemsTabs } from "./ProblemsTabs";

describe("вкладка «Модели»", () => {
  it("модели в порядке объявления, у каждой имя и размер в клетках: ширина × глубина × высота", () => {
    const html = renderToStaticMarkup(
      <ModelsPanel
        models={[
          { name: "house", size: [8, 6, 7] },
          { name: "birch", size: [2.456, 2.5, 9] },
        ]}
      />,
    );

    expect(html.indexOf(">house<")).toBeLessThan(html.indexOf(">birch<"));
    expect(html).toContain(">8 × 6 × 7<");
    expect(html).toContain(">2.46 × 2.5 × 9<");
    expect(html.match(/draggable="true"/g)).toHaveLength(2);
  });

  it("моделей нет — текст «В игре нет моделей»", () => {
    expect(renderToStaticMarkup(<ModelsPanel models={[]} />)).toContain("В игре нет моделей");
  });
});

describe("вкладка «Модели» в нижней панели", () => {
  const render = (models: Parameters<typeof ProblemsTabs>[0]["models"]): string =>
    renderToStaticMarkup(
      <ProblemsTabs errorLines={[]} warningLines={[]} isLoading={false} stepReport={undefined} messages={[]} imageTiles={[]} particlesPanel={null} models={models} onSelectObject={() => {}} />,
    );

  it("в трёхмерной сцене стоит после «Картинок»", () => {
    const html = render([]);

    expect(html.indexOf(">Картинки")).toBeLessThan(html.indexOf(">Модели"));
  });

  it("в плоской сцене вкладки нет", () => {
    expect(render(null)).not.toContain(">Модели");
  });
});
