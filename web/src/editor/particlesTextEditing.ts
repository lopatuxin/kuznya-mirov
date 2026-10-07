import { findNodeAtLocation, parseTree, type JSONPath } from "jsonc-parser";
import type { ParticleFields } from "./particlesFile";
import { parseSceneObjects } from "./sceneObjects";
import { appendLastKey, editAt, formatSceneValue, removeObjectProperty, setObjectPropertyValue } from "./sceneTextEditing";

function lineBreakOf(text: string): string {
  return text.includes("\r\n") ? "\r\n" : "\n";
}

/** Первое свободное имя: само `base`, иначе `base-2`, `base-3` и так далее. */
function freeParticleKindName(base: string, takenNames: readonly string[]): string {
  const taken = new Set(takenNames);
  for (let attempt = 1; ; attempt += 1) {
    const name = attempt === 1 ? base : `${base}-${attempt}`;
    if (!taken.has(name)) return name;
  }
}

/** Имя копии вида — «<имя>-копия», занятое — «<имя>-копия-2» и дальше. */
export function copiedParticleKindName(name: string, takenNames: readonly string[]): string {
  return freeParticleKindName(`${name}-копия`, takenNames);
}

/**
 * Убирает свойство по пути вместе с запятой, на которой оно держалось: соседи остаются на своих строках и со своими
 * отступами, остальной текст — байт в байт.
 */
function removeAt(text: string, path: JSONPath): string {
  const root = parseTree(text);
  const property = root === undefined ? undefined : findNodeAtLocation(root, path)?.parent;
  const container = property?.parent;
  if (property === undefined || property.type !== "property" || container?.type !== "object") return text;
  const siblings = container.children ?? [];
  const index = siblings.indexOf(property);
  const end = (node: { offset: number; length: number }): number => node.offset + node.length;
  if (siblings.length === 1) return text.slice(0, container.offset + 1) + text.slice(end(container) - 1);
  const next = siblings[index + 1];
  if (next !== undefined) return text.slice(0, property.offset) + text.slice(next.offset);
  return text.slice(0, end(siblings[index - 1] as { offset: number; length: number })) + text.slice(end(property));
}

/**
 * Значение поля вида на месте — «Редактор», требование 28: заменяется только оно; ключа не было — дописывается
 * последним в свой вид; `undefined` убирает ключ. Вида с таким именем нет — текст не меняется.
 */
export function particlesTextWithValue(text: string, name: string, key: string, value: unknown): string {
  const root = parseTree(text);
  if (root === undefined || findNodeAtLocation(root, [name]) === undefined) return text;
  const hasKey = findNodeAtLocation(root, [name, key]) !== undefined;
  if (value === undefined) return hasKey ? removeAt(text, [name, key]) : text;
  return hasKey ? editAt(text, [name, key], value, false) : appendLastKey(text, [name], key, value);
}

/**
 * Новый вид последним в файле в стиле файла — «Редактор», требование 28. В пустой таблице вид встаёт на свою строку:
 * дальше следующие виды ложатся под него.
 */
export function particlesTextWithKind(text: string, name: string, fields: ParticleFields): string {
  const root = parseTree(text);
  if (root?.type === "object" && (root.children ?? []).length === 0) {
    const lineBreak = lineBreakOf(text);
    const table = `{${lineBreak}  ${JSON.stringify(name)}: ${formatSceneValue(fields)}${lineBreak}}`;
    return text.slice(0, root.offset) + table + text.slice(root.offset + root.length);
  }
  return appendLastKey(text, [], name, fields);
}

/** Вид пропадает из файла вместе со своей запятой — «Редактор», требование 31. */
export function particlesTextWithoutKind(text: string, name: string): string {
  return removeAt(text, [name]);
}

/** Вид переименован на своём месте: меняется только его ключ — «Редактор», требование 32. */
export function particlesTextWithRenamedKind(text: string, name: string, newName: string): string {
  const root = parseTree(text);
  const keyNode = root?.children?.find((property) => property.children?.[0]?.value === name)?.children?.[0];
  if (keyNode === undefined) return text;
  return text.slice(0, keyNode.offset) + JSON.stringify(newName) + text.slice(keyNode.offset + keyNode.length);
}

/** Номера объектов сцены, которые называют вид. */
function sceneObjectIndexesUsing(sceneText: string, name: string): number[] {
  return parseSceneObjects(sceneText).flatMap((object, index) =>
    object !== null && typeof object === "object" && (object as Record<string, unknown>).particles === name ? [index] : [],
  );
}

/** Источники сцены, названные старым именем вида, получают новое — переименование идёт одним действием с файлом видов. */
export function sceneTextWithRenamedParticles(sceneText: string, name: string, newName: string): string {
  return sceneObjectIndexesUsing(sceneText, name).reduce((text, index) => setObjectPropertyValue(text, index, "particles", newName), sceneText);
}

/** У источников сцены, названных удалённым видом, `particles` убирается — иначе сцена пропала бы за ошибкой. */
export function sceneTextWithoutParticles(sceneText: string, name: string): string {
  return sceneObjectIndexesUsing(sceneText, name).reduce((text, index) => removeObjectProperty(text, index, "particles"), sceneText);
}
