import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { HandleModeToolbar } from "./HandleModeToolbar";

type ToolbarProps = Parameters<typeof HandleModeToolbar>[0];

const NOOP = (): void => {};

function toolbarProps(overrides: Partial<ToolbarProps> = {}, paintOverrides: Partial<ToolbarProps["paintTool"]> = {}, blocked: string[] = []): ToolbarProps {
  return {
    mode: "translate",
    brushKind: null,
    areBrushesAvailable: true,
    brushSize: 4,
    brushStrength: 50,
    water: null,
    imprintTool: {
      isSelected: false,
      isEnabled: true,
      fields: { stampNames: ["beluha"], stamp: "beluha", width: 30, height: 10, onStampChange: NOOP, onWidthChange: NOOP, onHeightChange: NOOP },
      onSelect: NOOP,
    },
    paintTool: {
      isSelected: false,
      isEnabled: true,
      fields: {
        size: 4,
        strength: 50,
        materialNames: ["grass", "rock", "scree"],
        material: "grass",
        blockedMaterials: new Set(blocked),
        onSizeChange: NOOP,
        onStrengthChange: NOOP,
        onMaterialChange: NOOP,
      },
      onSelect: NOOP,
      ...paintOverrides,
    },
    onChange: NOOP,
    onBrushChange: NOOP,
    onBrushSizeChange: NOOP,
    onBrushStrengthChange: NOOP,
    onWaterChange: NOOP,
    ...overrides,
  };
}

function render(props: ToolbarProps): string {
  return renderToStaticMarkup(<HandleModeToolbar {...props} />);
}

/** Тег кнопки с подсказкой `title`. */
function buttonTagOf(html: string, title: string): string {
  const tag = html.match(new RegExp(`<button[^>]*title="${title}"[^>]*>`))?.[0];
  if (tag === undefined) throw new Error(`кнопки «${title}» нет`);
  return tag;
}

describe("кнопка «Покрасить»", () => {
  it("группа «Покраска» с подписью, одна кнопка со значком кисти, после группы «Рельеф»", () => {
    const html = render(toolbarProps());

    expect(html).toContain('role="group" aria-label="Покраска"');
    expect(html).toContain(">Покраска</span>");
    expect(html.indexOf('aria-label="Кисти рельефа"')).toBeLessThan(html.indexOf('aria-label="Покраска"'));
    const group = html.slice(html.indexOf('aria-label="Покраска"'));
    expect((group.slice(0, group.indexOf("</div>")).match(/<button/g) ?? []).length).toBe(1);
  });

  it("кнопка нажимается и не забирает фокус у сцены: подсказка-название и кнопка не выбрана", () => {
    const tag = buttonTagOf(render(toolbarProps()), "Покрасить землю материалом \\(с Shift — стереть\\)");

    expect(tag).not.toContain("disabled");
    expect(tag).toContain('aria-pressed="false"');
  });

  it("выбранная кнопка нажата, а виды ручек — нет", () => {
    const html = render(toolbarProps({}, { isSelected: true }));

    expect(buttonTagOf(html, "Покрасить землю материалом \\(с Shift — стереть\\)")).toContain('aria-pressed="true"');
    expect(buttonTagOf(html, "Перенос \\(W\\)")).toContain('aria-pressed="false"');
  });

  it("без files.materials кнопка неактивна, подсказка называет, что объявить", () => {
    const html = render(toolbarProps({}, { isEnabled: false }));

    const tag = buttonTagOf(html, "Нет материалов: объяви files.materials в game.json");
    expect(tag).toContain("disabled");
  });

  it("в партии, на паузе, в повторе и в плоской сцене кистей нет: ни группы, ни кнопки, ни полей", () => {
    const html = render(toolbarProps({ areBrushesAvailable: false }, { isSelected: true }));

    expect(html).not.toContain("Покраска");
    expect(html).not.toContain("Материал");
  });
});

describe("поля кисти «Покрасить»", () => {
  it("группа «Кисть»: «Размер» и «Сила» с общими значениями и «Материал»; группы «Вода» нет", () => {
    const html = render(toolbarProps({}, { isSelected: true }));

    expect(html).toContain('role="group" aria-label="Кисть"');
    expect(html).toMatch(/Размер<input[^>]*value="4"/);
    expect(html).toMatch(/Сила<input[^>]*value="50"/);
    expect(html).toContain("Материал<select");
    expect(html).not.toContain('aria-label="Вода"');
  });

  it("материалы — в порядке объявления, выбран первый", () => {
    const html = render(toolbarProps({}, { isSelected: true }));

    const options = Array.from(html.matchAll(/<option value="([^"]*)"( selected="")?/g)).map((match) => [match[1], match[2] !== undefined]);
    expect(options.filter(([name]) => ["grass", "rock", "scree"].includes(name as string))).toEqual([
      ["grass", true],
      ["rock", false],
      ["scree", false],
    ]);
  });

  it("слоёв восемь — материалы, которых нет среди слоёв, неактивны с припиской «— слоёв уже восемь»", () => {
    const html = render(toolbarProps({}, { isSelected: true }, ["scree"]));

    expect(html).toContain('<option value="scree" disabled="">scree — слоёв уже восемь</option>');
    expect(html).toContain('<option value="grass" selected="">grass</option>');
    expect(html).toContain('<option value="rock">rock</option>');
  });

  it("пока выбрана не «Покрасить», полей материала нет", () => {
    const html = render(toolbarProps());

    expect(html).not.toContain("Материал");
  });

  it("у кисти рельефа поля — «Размер», «Сила» и группа «Вода», без «Материала»", () => {
    const html = render(toolbarProps({ brushKind: "raise" }));

    expect(html).toContain('aria-label="Вода"');
    expect(html).not.toContain("Материал");
  });
});

describe("кнопка «Отпечаток»", () => {
  it("в группе «Рельеф» после кистей, со значком холма и впадины, не выбрана; поля — только когда выбрана", () => {
    const html = render(toolbarProps());

    const group = html.slice(html.indexOf('aria-label="Кисти рельефа"'));
    const groupHtml = group.slice(0, group.indexOf("</div>"));
    expect((groupHtml.match(/<button/g) ?? []).length).toBe(4);
    expect(groupHtml.lastIndexOf('title="Отпечаток"')).toBeGreaterThan(groupHtml.lastIndexOf('title="Сгладить перепады"'));
    expect(groupHtml).toContain('d="M2 12c1.5-4.5 3-7 5-7s3.5 2.5 5 7"');
    expect(groupHtml).toContain('d="M12 12c1.5 4.5 3 7 5 7s3.5-2.5 5-7"');
    const tag = buttonTagOf(html, "Отпечаток");
    expect(tag).not.toContain("disabled");
    expect(tag).toContain('aria-pressed="false"');
    expect(html).not.toContain("Меньше нуля — вдавливает");
    expect(html).not.toContain("Гора");
  });

  it("выбранная — нажата, и появляются поля с подсказкой «Меньше нуля — вдавливает»", () => {
    const html = render(toolbarProps({ imprintTool: { ...toolbarProps().imprintTool, isSelected: true } }));

    expect(buttonTagOf(html, "Отпечаток")).toContain('aria-pressed="true"');
    expect(html).toContain('role="group" aria-label="Отпечаток"');
    expect(html).toContain('title="Меньше нуля — вдавливает"');
  });

  it("без files.stamps кнопка неактивна, подсказка называет, что объявить", () => {
    const html = render(toolbarProps({ imprintTool: { ...toolbarProps().imprintTool, isEnabled: false } }));

    expect(buttonTagOf(html, "Нет штампов: объяви files.stamps в game.json")).toContain("disabled");
  });
});
