import { getNodeValue, parseTree } from "jsonc-parser";
import type { EngineEditResult } from "./battleTypes";
import type { WindEditor } from "./sceneWind";

/** Поля одного вида частиц, как они стоят в `particles.json`: ключи файла и значения без правки. */
export type ParticleFields = Readonly<Record<string, unknown>>;

/** Вид частиц файла: имя и его поля. */
export type ParticleKind = { name: string; fields: ParticleFields };

/** Таблица «имя → вид», как её принимает `set_wind_particles` и читает `particles.json`. */
export type ParticleTable = Readonly<Record<string, ParticleFields>>;

/**
 * Виды из текста `particles.json` в порядке файла — «Ветер и частицы»: порядок карточек вкладки берётся из текста,
 * потому что движок отдаёт виды по алфавиту. Текст не читается, не объект — видов нет; вид, который не объект, —
 * вид без полей: карточка видна, а проверку его ошибки делает движок.
 */
export function readParticleKinds(text: string | null): ParticleKind[] {
  if (text === null) return [];
  const root = parseTree(text);
  if (root === undefined || root.type !== "object") return [];
  return (root.children ?? []).flatMap((property): ParticleKind[] => {
    const [keyNode, valueNode] = property.children ?? [];
    if (keyNode === undefined || valueNode === undefined) return [];
    const value: unknown = getNodeValue(valueNode);
    const isFieldsObject = valueNode.type === "object" && value !== null && typeof value === "object";
    return [{ name: String(keyNode.value), fields: isFieldsObject ? (value as ParticleFields) : {} }];
  });
}

/** Таблица видов для движка из показанных видов. */
export function particleTableOf(kinds: readonly ParticleKind[]): ParticleTable {
  return Object.fromEntries(kinds.map((kind) => [kind.name, kind.fields]));
}

/** Те же виды, у которых поле `key` вида `name` заменено значением; `undefined` убирает ключ. */
export function particleKindsWithValue(kinds: readonly ParticleKind[], name: string, key: string, value: unknown): ParticleKind[] {
  return kinds.map((kind) => {
    if (kind.name !== name) return kind;
    const fields = { ...kind.fields };
    if (value === undefined) delete fields[key];
    else fields[key] = value;
    return { name, fields };
  });
}

/**
 * Ставит виды собранному или живому миру движка; текст ошибки по-русски, если движок их не принял, иначе `undefined` —
 * «Редактор», «Вызовы движка». Файлы вызов не пишет.
 */
export function applyParticles(editor: WindEditor, table: ParticleTable): string | undefined {
  const result = editor.set_wind_particles({ particles: table }) as EngineEditResult;
  return result.ok ? undefined : result.error;
}

/**
 * Какие виды частиц объявляют ошибку проверки — «Редактор», требование 36: имя вида — первое звено пути ошибки
 * (`дым → rate`) в файле видов; карточка такого вида помечена, а поля правятся.
 */
export function invalidParticleKindNames(errors: readonly { file: string; path: string }[], particlesPath: string | null): Set<string> {
  if (particlesPath === null) return new Set();
  return new Set(errors.filter((error) => error.file === particlesPath && error.path !== "").map((error) => error.path.split(" → ")[0] as string));
}

/**
 * Ошибки проверки загрузкой по полям видов — «Редактор», требования 26, 27 и 36: вид — первое звено пути ошибки в файле
 * видов, поле — второе; у поля остаётся первая из его ошибок. Ошибка на весь вид (путь — одно имя) идёт под ключом `""`.
 */
export function particleFieldErrorsByKind(errors: readonly { file: string; path: string; message: string }[], particlesPath: string | null): Map<string, Record<string, string>> {
  const byKind = new Map<string, Record<string, string>>();
  if (particlesPath === null) return byKind;
  for (const error of errors) {
    const [kindName, key] = error.path.split(" → ");
    if (error.file !== particlesPath || kindName === undefined || kindName === "") continue;
    const fields = byKind.get(kindName) ?? {};
    const field = key ?? "";
    if (!Object.hasOwn(fields, field)) fields[field] = error.message;
    byKind.set(kindName, fields);
  }
  return byKind;
}
