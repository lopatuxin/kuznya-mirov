export type ParticleEffectId = "smoke" | "sparks" | "leaves";

export type ParticleEffect = {
  id: ParticleEffectId;
  /** Подпись карточки вкладки. */
  cardLabel: string;
  /** Заголовок группы справа. */
  title: string;
  /** Главное свойство: эффект идёт, пока оно больше нуля. */
  mainKey: string;
  /** Главное свойство и настройки эффекта — всё, что «Убрать» снимает. */
  keys: readonly string[];
  /** Какое значение главного свойства получает источник, на который карточку отпустили. */
  dropValue: number;
  /** Подписи концов ползунка плотности. */
  densityLabels: readonly [string, string];
};

/** Три эффекта вкладки «Частицы» в порядке карточек и групп — «Редактор», требования 28–29. */
export const PARTICLE_EFFECTS: readonly ParticleEffect[] = [
  { id: "smoke", cardLabel: "дым", title: "Дым", mainKey: "smoke", keys: ["smoke", "smoke_height", "smoke_color"], dropValue: 0.5, densityLabels: ["струйка", "густой столб"] },
  { id: "sparks", cardLabel: "искры", title: "Искры", mainKey: "sparks", keys: ["sparks", "sparks_reach", "sparks_direction", "sparks_spread"], dropValue: 0.5, densityLabels: ["редкие", "густые"] },
  { id: "leaves", cardLabel: "листья", title: "Листья", mainKey: "leaf_fall", keys: ["leaf_fall", "leaf_color"], dropValue: 0.3, densityLabels: ["изредка", "сильный"] },
];

/** Девять встроенных свойств частиц плоской сцены. */
export const PARTICLE_PROPERTY_NAMES: readonly string[] = PARTICLE_EFFECTS.flatMap((effect) => effect.keys);

/** Свойства-цвета частиц: правятся палитрой, как `color`. */
export const PARTICLE_COLOR_NAMES: readonly string[] = ["smoke_color", "leaf_color"];

/** Что движок берёт без свойства — «Ветер и частицы», «Дым и искры»; во вкладке это бледное значение поля. */
export const PARTICLE_DEFAULTS = { smoke_height: 4, sparks_reach: 1.5, sparks_direction: 0, sparks_spread: 30, smoke_color: "#a6a6ac" } as const;

/** Русские подписи свойств частиц: так ключи называются во вкладке «Частицы» — у полей и в тексте ошибки под ними. */
export const PARTICLE_LABELS = {
  smoke: "плотность",
  sparks: "плотность",
  leaf_fall: "плотность",
  smoke_color: "цвет",
  smoke_height: "высота столба, клеток",
  sparks_reach: "как далеко летят, клеток",
  sparks_direction: "направление",
  sparks_spread: "разброс",
  leaf_color: "цвет листьев",
} as const;

const PARTICLE_KEY_PATTERN = new RegExp(String.raw`\b(${Object.keys(PARTICLE_LABELS).join("|")})\b`, "g");

/** Текст ошибки движка без английских ключей: каждый ключ свойства частиц заменён русской подписью поля. */
export function translateParticleKeys(message: string): string {
  return message.replace(PARTICLE_KEY_PATTERN, (key) => PARTICLE_LABELS[key as keyof typeof PARTICLE_LABELS]);
}

/** Цвет, который получают листья, когда снимают «осенние вперемешку»: палитре нужен цвет, а не его отсутствие. */
export const LEAF_COLOR_START = "#d9a531";

export const DENSITY_STEP = 0.05;

export const MAX_SPARKS_SPREAD = 180;

const DIRECTION_STEP = 15;

/** Эффект по ключу карточки; `undefined` — такого эффекта нет. */
export function particleEffectById(id: string): ParticleEffect | undefined {
  return PARTICLE_EFFECTS.find((effect) => effect.id === id);
}

/** Эффекты объекта в порядке «Дым», «Искры», «Листья»: у кого есть главное свойство, даже если оно 0 — группа остаётся, пока плотность тянут до нуля. */
export function effectsOfObject(properties: Readonly<Record<string, unknown>> | null): ParticleEffect[] {
  if (properties === null) return [];
  return PARTICLE_EFFECTS.filter((effect) => typeof properties[effect.mainKey] === "number");
}

/** Ключи эффекта, которые у объекта есть, — их снимает «Убрать». */
export function presentEffectKeys(effect: ParticleEffect, properties: Readonly<Record<string, unknown>>): string[] {
  return effect.keys.filter((key) => key in properties);
}

/** Число, как его пишут по-русски: дробная часть через запятую. */
export function formatParticleNumber(value: number): string {
  return String(value).replace(".", ",");
}

/** Градусы по кругу в отрезок от 0 до 360 (не включая его); 450 — то же, что 90. */
export function normalizeDegrees(degrees: number): number {
  return ((degrees % 360) + 360) % 360;
}

/** Угол точки относительно центра круга: 0 — вверх, 90 — вправо, по часовой стрелке, от 0 до 360. */
export function degreesAtPoint(dx: number, dy: number): number {
  return normalizeDegrees((Math.atan2(dx, -dy) * 180) / Math.PI);
}

/** Целые градусы; с Ctrl — шагом 15°. */
function roundDegrees(degrees: number, isStepped: boolean): number {
  return isStepped ? Math.round(degrees / DIRECTION_STEP) * DIRECTION_STEP : Math.round(degrees);
}

/** Новое направление искр, когда тянут конец стрелки к точке `(dx, dy)` от центра круга. */
export function sparksDirectionAtPoint(dx: number, dy: number, isStepped: boolean): number {
  return normalizeDegrees(roundDegrees(degreesAtPoint(dx, dy), isStepped));
}

/** Новый разброс искр, когда тянут край веера к точке `(dx, dy)`: насколько она отстоит от направления, от 0 до 180. */
export function sparksSpreadAtPoint(dx: number, dy: number, direction: number, isStepped: boolean): number {
  const offset = Math.abs(normalizeDegrees(degreesAtPoint(dx, dy) - direction + 180) - 180);
  return Math.min(MAX_SPARKS_SPREAD, roundDegrees(offset, isStepped));
}

const DIRECTION_WORDS = ["вверх", "вверх-вправо", "вправо", "вниз-вправо", "вниз", "вниз-влево", "влево", "вверх-влево"] as const;

/** Направление и разброс словами под кругом — «Редактор», требование 29: «вверх, ±30°»; между сторонами — градусы по часовой от «вверх». */
export function describeSparksFan(direction: number, spread: number): string {
  const normalized = normalizeDegrees(Math.round(direction));
  const word = normalized % 45 === 0 ? (DIRECTION_WORDS[normalized / 45] as string) : `${normalized}°`;
  return `${word}, ±${Math.round(spread)}°`;
}

/** Цвет для палитры браузера: только `#rrggbb`, иначе запасной. */
export function colorForPicker(value: unknown, fallback: string): string {
  return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
}
