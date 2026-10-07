import { beginAction, beginParticlesCreation, type EditSessionState, type EditSnapshot } from "./editSession";
import { builtInParticleFields } from "./particlePresets";
import { readParticleKinds, type ParticleFields } from "./particlesFile";
import {
  copiedParticleKindName,
  particlesTextWithKind,
  particlesTextWithoutKind,
  particlesTextWithRenamedKind,
  particlesTextWithValue,
  sceneTextWithoutParticles,
  sceneTextWithRenamedParticles,
} from "./particlesTextEditing";
import { chooseParticlesFilePath, parseProjectFilePaths } from "./projectFiles";
import { parseSceneObjects } from "./sceneObjects";
import { addParticlesFilePath, appendSceneObject } from "./sceneTextEditing";

/** Пустая таблица видов: ею начинается файл, который завёл первый вид, и к ней возвращает отмена («Редактор», требование 29). */
export const EMPTY_PARTICLES_TEXT = "{}\n";

/** Новые тексты файлов после действия вкладки и вид, который встаёт выбранным; `null` — выбор сам найдёт первый вид. */
export type ParticlesChange = { snapshot: EditSnapshot; selected: string | null };

/** Что действие вкладки делает с правкой: новое состояние, `game.json` с `files.particles` и выбранный вид. */
type ParticlesEditPlan = { state: EditSessionState; gameJsonText: string; selected: string | null };

/**
 * Действие вкладки «Частицы» — «Редактор», требования 28–33: `build` считает новые тексты от показанных. У проекта с
 * файлом видов это обычное действие правки. У проекта без файла первое действие заводит его: имя `particles.json`
 * (или `particles-2.json` и дальше, если занято), ключ `files.particles` дописан в `game.json`, а отмена вернёт пустую
 * таблицу, не «файла нет». `null` — действия нет: нечего править или тексты не изменились.
 */
export async function planParticlesEdit(
  session: EditSessionState,
  gameJsonText: string,
  isFilePresent: (relativePath: string) => Promise<boolean>,
  build: (displayed: EditSnapshot) => ParticlesChange | null,
): Promise<ParticlesEditPlan | null> {
  if (parseProjectFilePaths(gameJsonText) === null) return null;
  const hasFile = session.displayed.particlesText !== null;
  const base: EditSnapshot = hasFile ? session.displayed : { ...session.displayed, particlesText: EMPTY_PARTICLES_TEXT };
  const change = build(base);
  if (change === null) return null;
  const { snapshot, selected } = change;
  if (hasFile) {
    const isUnchanged = snapshot.particlesText === session.displayed.particlesText && snapshot.sceneText === session.displayed.sceneText;
    return isUnchanged ? null : { state: beginAction(session, snapshot), gameJsonText, selected };
  }
  const path = await chooseParticlesFilePath(isFilePresent);
  return { state: beginParticlesCreation(session, snapshot, EMPTY_PARTICLES_TEXT), gameJsonText: addParticlesFilePath(gameJsonText, path), selected };
}

function existingText(displayed: EditSnapshot): string {
  return displayed.particlesText ?? EMPTY_PARTICLES_TEXT;
}

function namesOf(displayed: EditSnapshot): string[] {
  return readParticleKinds(displayed.particlesText).map((kind) => kind.name);
}

/** Поля вида: из файла, а готового вида, которого в файле ещё нет, — его заготовка; `undefined` — такого вида нет. */
function kindFields(displayed: EditSnapshot, name: string): { fields: ParticleFields; isInFile: boolean } | undefined {
  const own = readParticleKinds(displayed.particlesText).find((candidate) => candidate.name === name);
  if (own !== undefined) return { fields: own.fields, isInFile: true };
  const preset = builtInParticleFields(name);
  return preset === undefined ? undefined : { fields: preset, isInFile: false };
}

function fieldsWithValue(fields: ParticleFields, key: string, value: unknown): ParticleFields {
  const next: Record<string, unknown> = { ...fields };
  if (value === undefined) delete next[key];
  else next[key] = value;
  return next;
}

/**
 * Поле вида: `undefined` убирает ключ; то же значение — действия нет. Готовый вид, которого в файле ещё нет, первой правкой
 * записывается в файл целиком — заготовка с этим значением.
 */
export function particleValueChange(displayed: EditSnapshot, name: string, key: string, value: unknown): ParticlesChange | null {
  const kind = kindFields(displayed, name);
  if (kind === undefined || JSON.stringify(kind.fields[key]) === JSON.stringify(value)) return null;
  const particlesText = kind.isInFile
    ? particlesTextWithValue(existingText(displayed), name, key, value)
    : particlesTextWithKind(existingText(displayed), name, fieldsWithValue(kind.fields, key, value));
  return { snapshot: { ...displayed, particlesText }, selected: name };
}

/**
 * Вид перетащили на сцену — «Редактор», требование 34: в конец `objects` встаёт источник, а готовый вид, которого в файле
 * ещё нет, записывается в файл — одно действие над двумя файлами.
 */
export function particleSourceChange(displayed: EditSnapshot, name: string, source: Record<string, unknown>): ParticlesChange | null {
  const kind = kindFields(displayed, name);
  if (kind === undefined) return null;
  const particlesText = kind.isInFile ? existingText(displayed) : particlesTextWithKind(existingText(displayed), name, kind.fields);
  const sceneText = appendSceneObject(displayed.sceneText, parseSceneObjects(displayed.sceneText).length, source);
  return { snapshot: { ...displayed, particlesText, sceneText }, selected: name };
}

/** «Копировать»: копия вида последней в файле; у готового вида, которого в файле ещё нет, — копия заготовки. */
export function particleCopyChange(displayed: EditSnapshot, name: string): ParticlesChange | null {
  const kind = kindFields(displayed, name);
  if (kind === undefined) return null;
  const copyName = copiedParticleKindName(name, namesOf(displayed));
  return { snapshot: { ...displayed, particlesText: particlesTextWithKind(existingText(displayed), copyName, kind.fields) }, selected: copyName };
}

/** «Удалить»: вид пропадает из файла, а у источников сцены `particles` убирается — одно действие над двумя файлами. */
export function particleDeleteChange(displayed: EditSnapshot, name: string): ParticlesChange | null {
  const names = namesOf(displayed);
  const index = names.indexOf(name);
  if (index === -1) return null;
  const remaining = names.filter((candidate) => candidate !== name);
  return {
    snapshot: { ...displayed, particlesText: particlesTextWithoutKind(existingText(displayed), name), sceneText: sceneTextWithoutParticles(displayed.sceneText, name) },
    selected: remaining[index] ?? remaining.at(-1) ?? null,
  };
}

/** Переименование: вид на своём месте в файле видов и `particles` у источников сцены — одно действие над двумя файлами. */
export function particleRenameChange(displayed: EditSnapshot, name: string, newName: string): ParticlesChange | null {
  if (!namesOf(displayed).includes(name)) return null;
  return {
    snapshot: {
      ...displayed,
      particlesText: particlesTextWithRenamedKind(existingText(displayed), name, newName),
      sceneText: sceneTextWithRenamedParticles(displayed.sceneText, name, newName),
    },
    selected: newName,
  };
}

/** Что не так с новым именем вида — «Редактор», требование 32: пустое имя или имя другого вида; `undefined` — имя годится. */
export function particleKindNameError(newName: string, name: string, kindNames: readonly string[]): string | undefined {
  if (newName === "") return "Имя вида не может быть пустым";
  if (newName !== name && kindNames.includes(newName)) return `Вид «${newName}» уже есть`;
  return undefined;
}
