import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ImagesPanel } from "./ImagesPanel";
import { ProblemsTabs } from "./ProblemsTabs";
import type { ProjectImageTile } from "./projectImages";

function tile(name: string, isReadable: boolean): ProjectImageTile {
  return {
    description: { name, frames: null, columns: null, size: null, smooth: false },
    image: isReadable ? { name, width: 4, height: 4, pixels: new Uint8Array(64) } : null,
  };
}

describe("вкладка «Картинки»", () => {
  it("картинки в порядке объявления, у каждой уменьшенный кадр в квадрате 64 точки и имя под ним", () => {
    const html = renderToStaticMarkup(<ImagesPanel tiles={[tile("izba", true), tile("birch", true)]} />);

    expect(html.indexOf(">izba<")).toBeLessThan(html.indexOf(">birch<"));
    expect(html.match(/<canvas[^>]*width="64"[^>]*height="64"/g)).toHaveLength(2);
  });

  it("читаемую картинку можно перетащить, нечитаемую — нет: у неё имя и пустой серый квадрат", () => {
    const html = renderToStaticMarkup(<ImagesPanel tiles={[tile("izba", true), tile("broken", false)]} />);

    const items = html.split("<li").slice(1);
    expect(items[0]).toContain('draggable="true"');
    expect(items[1]).toContain('draggable="false"');
    expect(items[1]).toContain("images-panel__item--unreadable");
    expect(items[1]).toContain("images-panel__thumb--empty");
    expect(items[1]).not.toContain("<canvas");
    expect(items[1]).toContain(">broken<");
  });

  it("картинок нет — текст «В игре нет картинок»", () => {
    expect(renderToStaticMarkup(<ImagesPanel tiles={[]} />)).toContain("В игре нет картинок");
  });
});

describe("вкладка «Частицы» в нижней панели", () => {
  const render = (particlesPanel: React.ReactNode | null): string =>
    renderToStaticMarkup(<ProblemsTabs errorLines={[]} warningLines={[]} isLoading={false} stepReport={undefined} messages={[]} imageTiles={[]} particlesPanel={particlesPanel} onSelectObject={() => {}} />);

  it("в плоской сцене стоит после «Картинок»", () => {
    const html = render(<span />);

    expect(html.indexOf(">Картинки")).toBeLessThan(html.indexOf(">Частицы"));
  });

  it("в трёхмерной сцене вкладки нет", () => {
    expect(render(null)).not.toContain(">Частицы");
  });
});

describe("нижняя панель", () => {
  it("после «Сообщений» — вкладка «Картинки»", () => {
    const html = renderToStaticMarkup(
      <ProblemsTabs errorLines={[]} warningLines={[]} isLoading={false} stepReport={undefined} messages={[]} imageTiles={[]} particlesPanel={null} onSelectObject={() => {}} />,
    );

    expect(html.indexOf(">Сообщения")).toBeLessThan(html.indexOf(">Картинки"));
  });
});
