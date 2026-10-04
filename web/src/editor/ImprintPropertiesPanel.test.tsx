import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ImprintPropertiesPanel } from "./ImprintPropertiesPanel";

const NOOP = (): void => {};

describe("свойства отпечатка", () => {
  it("заголовок «Отпечаток N» — номер в stamps с единицы; height со знаком как в файле", () => {
    const html = renderToStaticMarkup(
      <ImprintPropertiesPanel
        index={2}
        entry={{ stamp: "ravine", position: [10, 8], size: [20, 10], height: -3 }}
        stampNames={["hill", "ravine"]}
        canEdit
        onSetValue={NOOP}
        onCopy={NOOP}
        onDelete={NOOP}
      />,
    );

    expect(html).toContain("Отпечаток 3");
    expect(html).not.toContain("Гора");
    expect(html).toMatch(/height<\/dt><dd[^>]*><input[^>]*value="-3"/);
  });
});
