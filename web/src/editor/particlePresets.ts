import type { ParticleFields, ParticleKind } from "./particlesFile";

/**
 * Готовые виды вкладки «Частицы» — дым, искры и листья, как встроенные частицы GameMaker: каждый рисует движок сам
 * (`shape`), без картинок игры, и каждый уже настроен. Во вкладке они есть всегда; в `particles.json` вид попадает при
 * первой правке поля или когда его перетащили на сцену.
 */
const BUILT_IN_PARTICLE_KINDS: readonly ParticleKind[] = [
  {
    name: "дым",
    fields: { shape: "smoke", rate: 12, lifetime: [4, 5.5], size: [0.45, 0.6], grow: 3.5, speed: [0.6, 0.9], spread: 12, spin: [-20, 20], opacity: [0.55, 0.35, 0] },
  },
  {
    name: "искры",
    fields: { shape: "spark", rate: 12, lifetime: [0.6, 1.2], size: [0.12, 0.2], speed: [1.5, 3], spread: 30, gravity: 2.5, wind: 0.3, opacity: [1, 1, 0] },
  },
  {
    name: "листья",
    fields: {
      shape: "leaf",
      rate: 1,
      lifetime: [6, 8],
      size: [0.18, 0.28],
      speed: [0.2, 0.4],
      direction: 180,
      spread: 60,
      gravity: 0.15,
      wobble: 0.5,
      spin: [-120, 120],
      wind: 0.8,
      opacity: [1, 1, 0],
    },
  },
];

/** Поля готового вида по имени; `undefined` — вид не готовый. */
export function builtInParticleFields(name: string): ParticleFields | undefined {
  return BUILT_IN_PARTICLE_KINDS.find((kind) => kind.name === name)?.fields;
}

/** Вид во вкладке: готовый или свой, записан ли он уже в `particles.json`. */
export type TabParticleKind = ParticleKind & { isBuiltIn: boolean; isInFile: boolean };

/** Виды вкладки: сначала готовые — из файла, если он их уже держит, иначе заготовкой, — дальше остальные виды файла по порядку. */
export function tabParticleKinds(fileKinds: readonly ParticleKind[]): TabParticleKind[] {
  const builtIns = BUILT_IN_PARTICLE_KINDS.map((preset) => {
    const own = fileKinds.find((kind) => kind.name === preset.name);
    return { name: preset.name, fields: own?.fields ?? preset.fields, isBuiltIn: true, isInFile: own !== undefined };
  });
  const others = fileKinds.filter((kind) => builtInParticleFields(kind.name) === undefined).map((kind) => ({ ...kind, isBuiltIn: false, isInFile: true }));
  return [...builtIns, ...others];
}
