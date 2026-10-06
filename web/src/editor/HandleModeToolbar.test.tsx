import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { HandleModeToolbar, MaterialsMenuContent, TerrainMenuContent, terrainGroupTool } from "./HandleModeToolbar";
import { WaterFields } from "./WaterFields";

type ToolbarProps = Parameters<typeof HandleModeToolbar>[0];
type TerrainMenuProps = Parameters<typeof TerrainMenuContent>[0];

const NOOP = (): void => {};

function toolbarProps(overrides: Partial<ToolbarProps> = {}, materialsOverrides: Partial<ToolbarProps["materialsTool"]> = {}): ToolbarProps {
  return {
    mode: "translate",
    isThreeDimensionalScene: true,
    brushKind: null,
    areBrushesAvailable: true,
    lastTerrainTool: "raise",
    brushSize: 4,
    brushStrength: 50,
    water: null,
    imprintTool: {
      isSelected: false,
      isEnabled: true,
      fields: { stampNames: ["beluha"], stamp: "beluha", previews: new Map(), width: 30, height: 10, onStampChange: NOOP, onWidthChange: NOOP, onHeightChange: NOOP },
      onSelect: NOOP,
    },
    materialsTool: {
      isSelected: false,
      isEnabled: true,
      materialNames: ["grass", "rock", "scree"],
      material: "grass",
      blockedMaterials: new Set(),
      onMaterialSelect: NOOP,
      onSelect: NOOP,
      ...materialsOverrides,
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

function renderTerrainMenu(overrides: Partial<TerrainMenuProps> = {}): string {
  const { brushKind, brushSize, brushStrength, imprintTool, onBrushSizeChange, onBrushStrengthChange } = toolbarProps();
  return renderToStaticMarkup(
    <TerrainMenuContent
      brushKind={brushKind}
      brushSize={brushSize}
      brushStrength={brushStrength}
      imprintTool={imprintTool}
      onBrushSizeChange={onBrushSizeChange}
      onBrushStrengthChange={onBrushStrengthChange}
      groupTool="raise"
      onSelect={NOOP}
      onPicked={NOOP}
      {...overrides}
    />,
  );
}

/** Тег кнопки с подсказкой `title`. */
function buttonTagOf(html: string, title: string): string {
  const tag = html.match(new RegExp(`<button[^>]*title="${title}"[^>]*>`))?.[0];
  if (tag === undefined) throw new Error(`кнопки «${title}» нет`);
  return tag;
}

describe("полоса инструментов", () => {
  it("виды ручек значками, затем «Рельеф», «Материалы» и «Вода» кнопками с окошком; полей в полосе нет", () => {
    const html = render(toolbarProps());

    expect(html).toContain('role="group" aria-label="Ручки объекта"');
    expect(html.indexOf(">Рельеф<")).toBeLessThan(html.indexOf(">Материалы<"));
    expect(html.indexOf(">Материалы<")).toBeLessThan(html.indexOf(">Вода<"));
    expect(html).toContain('aria-label="Рельеф: настройки"');
    expect(html).toContain('aria-label="Материалы: настройки"');
    expect(html).not.toContain("Покраска");
    expect(html).not.toContain("Размер");
    expect(html).not.toContain("Уровень");
    expect(html).not.toContain('role="dialog"');
  });

  it("щелчок по «Рельеф» включает последний инструмент группы — его и называет подсказка", () => {
    expect(buttonTagOf(render(toolbarProps()), "Рельеф: Поднять")).toContain('aria-pressed="false"');
    expect(buttonTagOf(render(toolbarProps({ lastTerrainTool: "smooth", brushKind: "smooth" })), "Рельеф: Сгладить")).toContain('aria-pressed="true"');
  });

  it("выбранный «Отпечаток» нажимает «Рельеф», а виды ручек — нет", () => {
    const html = render(toolbarProps({ lastTerrainTool: "imprint", imprintTool: { ...toolbarProps().imprintTool, isSelected: true } }));

    expect(buttonTagOf(html, "Рельеф: Отпечаток")).toContain('aria-pressed="true"');
    expect(buttonTagOf(html, "Перенос \\(W\\)")).toContain('aria-pressed="false"');
  });

  it("кисть материалом нажимает «Материалы», подсказка называет материал", () => {
    const html = render(toolbarProps({}, { isSelected: true, material: "rock" }));

    expect(buttonTagOf(html, "Красить материалом rock \\(с Shift — стереть\\)")).toContain('aria-pressed="true"');
    expect(buttonTagOf(html, "Перенос \\(W\\)")).toContain('aria-pressed="false"');
  });

  it("без files.materials «Материалы» неактивны, подсказка называет, что объявить", () => {
    const html = render(toolbarProps({}, { isEnabled: false }));

    expect(buttonTagOf(html, "Нет материалов: объяви files.materials в game.json")).toContain("disabled");
  });

  it("в партии, на паузе и в повторе — только виды ручек: рельеф там не правится", () => {
    const html = render(toolbarProps({ areBrushesAvailable: false }, { isSelected: true }));

    expect(html).toContain('aria-label="Ручки объекта"');
    expect(html).not.toContain("Рельеф");
    expect(html).not.toContain("Материалы");
    expect(html).not.toContain("Вода");
  });
});

describe("полоса плоской сцены", () => {
  it("две кнопки вида ручек — перенос и масштаб, без поворота, групп рельефа, материалов и воды", () => {
    const html = render(toolbarProps({ isThreeDimensionalScene: false, areBrushesAvailable: false }));

    expect(buttonTagOf(html, "Перенос \\(W\\)")).toContain('aria-pressed="true"');
    expect(buttonTagOf(html, "Масштаб \\(R\\)")).toContain('aria-pressed="false"');
    expect(html).not.toContain("Поворот");
    expect(html).not.toContain("Рельеф");
    expect(html).not.toContain("Материалы");
    expect(html).not.toContain("Вода");
    expect(html.match(/<button/g)).toHaveLength(2);
  });

  it("выбран масштаб — нажата кнопка масштаба", () => {
    const html = render(toolbarProps({ isThreeDimensionalScene: false, areBrushesAvailable: false, mode: "scale" }));

    expect(buttonTagOf(html, "Масштаб \\(R\\)")).toContain('aria-pressed="true"');
  });

  it("в трёхмерной сцене кнопок видов ручек три, с поворотом", () => {
    expect(render(toolbarProps())).toContain("Поворот (E)");
  });
});

describe("terrainGroupTool", () => {
  it("последний инструмент группы, а недоступный «Отпечаток» — «Поднять»", () => {
    expect(terrainGroupTool("level", true)).toBe("level");
    expect(terrainGroupTool("imprint", true)).toBe("imprint");
    expect(terrainGroupTool("imprint", false)).toBe("raise");
  });
});

describe("окошко «Рельеф»", () => {
  it("кисти и «Отпечаток» с названиями, нажата только выбранная кисть", () => {
    const html = renderTerrainMenu({ brushKind: "level" });

    for (const label of ["Поднять", "Выровнять", "Сгладить", "Отпечаток"]) expect(html).toContain(`>${label}</span>`);
    expect(html).toContain("Shift — опустить");
    expect(html.match(/aria-pressed="true"/g)).toHaveLength(1);
    expect(html).toMatch(/aria-pressed="true"[^>]*>(?:(?!<\/button>).)*Выровнять/);
  });

  it("у кисти — «Размер» с подсказкой про Ctrl+колесо и «Сила»", () => {
    const html = renderTerrainMenu();

    expect(html).toMatch(/title="Над сценой — Ctrl\+колесо">Размер<input[^>]*value="4"/);
    expect(html).toMatch(/Сила<input[^>]*value="50"/);
    expect(html).not.toContain("Штамп");
  });

  it("у «Отпечатка» — штамп, ширина и высота с подсказкой «Меньше нуля — вдавливает»", () => {
    const html = renderTerrainMenu({ groupTool: "imprint" });

    expect(html).toContain('role="group" aria-label="Отпечаток"');
    expect(html).toContain('role="radiogroup" aria-label="Штамп"');
    expect(html).toContain('title="Меньше нуля — вдавливает"');
    expect(html).not.toContain("Сила");
  });

  it("без files.stamps «Отпечаток» неактивен, подсказка называет, что объявить", () => {
    const html = renderTerrainMenu({ imprintTool: { ...toolbarProps().imprintTool, isEnabled: false } });

    expect(buttonTagOf(html, "Нет штампов: объяви files.stamps в game.json")).toContain("disabled");
  });
});

describe("окошко «Материалы»", () => {
  function renderMaterials(overrides: Partial<ToolbarProps["materialsTool"]> = {}): string {
    const { materialsTool, brushSize, brushStrength, onBrushSizeChange, onBrushStrengthChange } = toolbarProps({}, overrides);
    return renderToStaticMarkup(
      <MaterialsMenuContent
        materialsTool={materialsTool}
        brushSize={brushSize}
        brushStrength={brushStrength}
        onBrushSizeChange={onBrushSizeChange}
        onBrushStrengthChange={onBrushStrengthChange}
        onPicked={NOOP}
      />,
    );
  }

  it("материалы — в порядке объявления, под ними «Размер» и «Сила», общие с кистью рельефа", () => {
    const html = renderMaterials();

    const labels = Array.from(html.matchAll(/class="tool-menu__item-label">([^<]*)</g)).map((match) => match[1]);
    expect(labels).toEqual(["grass", "rock", "scree"]);
    expect(html).toMatch(/Размер<input[^>]*value="4"/);
    expect(html).toMatch(/Сила<input[^>]*value="50"/);
  });

  it("нажат материал, которым кисть красит сейчас; пока кисть не выбрана — ни один", () => {
    expect(renderMaterials().match(/aria-pressed="true"/g)).toBe(null);

    const html = renderMaterials({ isSelected: true, material: "rock" });
    expect(html.match(/aria-pressed="true"/g)).toHaveLength(1);
    expect(html).toMatch(/aria-pressed="true"[^>]*>(?:(?!<\/button>).)*rock/);
  });

  it("слоёв восемь — материалы, которых нет среди слоёв, неактивны с припиской «слоёв уже восемь»", () => {
    const html = renderMaterials({ blockedMaterials: new Set(["scree"]) });

    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>(?:(?!<\/button>).)*scree<\/span><span class="tool-menu__item-hint">слоёв уже восемь/);
    expect(html.match(/disabled=""/g)).toHaveLength(1);
  });
});

describe("окошко «Вода»", () => {
  it("без воды галочка снята, уровень и цвет неактивны", () => {
    const html = renderToStaticMarkup(<WaterFields water={null} onWaterChange={NOOP} />);

    expect(html).not.toMatch(/<input type="checkbox" checked=""/);
    expect(html).toMatch(/Уровень<input[^>]*disabled=""/);
  });

  it("с водой — значения из рельефа", () => {
    const html = renderToStaticMarkup(<WaterFields water={{ level: -2.2, color: "#3f7fd0" }} onWaterChange={NOOP} />);

    expect(html).toMatch(/<input type="checkbox" checked=""/);
    expect(html).toMatch(/Уровень<input[^>]*value="-2.2"/);
    expect(html).not.toMatch(/Уровень<input[^>]*disabled=""/);
  });
});
