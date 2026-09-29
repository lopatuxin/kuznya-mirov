export type Vec2 = readonly [number, number];

/**
 * Место объекта трёхмерной сцены, как его правят ручки: `position` и `size` в клетках, `height` и
 * `rotation` в файле могут отсутствовать (`null`), `hasShape` — у объекта есть `shape`, то есть у
 * него есть высота; плоский объект на земле высоты не имеет («Редактор», требование 13).
 */
export type ObjectPlacement = {
  position: Vec2;
  size: Vec2;
  height: number | null;
  rotation: number | null;
  hasShape: boolean;
};

/** Свойства объекта, которые пишут ручки, — по одному ключу файла. */
export type PlacementKey = "position" | "size" | "height" | "rotation";

/** Высота фигуры без `height` — «Редактор», требование 13. */
export const DEFAULT_SHAPE_HEIGHT = 1;

function readVec2(value: unknown): Vec2 | null {
  if (!Array.isArray(value) || value.length < 2) return null;
  const [x, y] = value as unknown[];
  return typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y) ? [x, y] : null;
}

function readNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** Место из свойств объекта в виде файла (текст `scene.json` или `object_properties`); нет `position` или `size` — `null`. */
export function readObjectPlacement(properties: Record<string, unknown> | null | undefined): ObjectPlacement | null {
  if (properties === null || properties === undefined) return null;
  const position = readVec2(properties.position);
  const size = readVec2(properties.size);
  if (position === null || size === null) return null;
  return {
    position,
    size,
    height: readNumber(properties.height),
    rotation: readNumber(properties.rotation),
    hasShape: typeof properties.shape === "string",
  };
}

export function placementCenter(placement: ObjectPlacement): [number, number] {
  return [placement.position[0] + placement.size[0] / 2, placement.position[1] + placement.size[1] / 2];
}

/** Высота фигуры для расчёта: записанная или единица; у плоского объекта высоты нет. */
export function effectiveHeight(placement: ObjectPlacement): number | null {
  if (!placement.hasShape) return null;
  return placement.height ?? DEFAULT_SHAPE_HEIGHT;
}

/** `rotation` в отрезке от 0 до 360, 360 не включая. */
export function normalizeRotation(degrees: number): number {
  return ((degrees % 360) + 360) % 360;
}

export type PlacementChange = { key: PlacementKey; value: unknown; previous: unknown };

function isSameVec2(first: Vec2, second: Vec2): boolean {
  return first[0] === second[0] && first[1] === second[1];
}

/**
 * Свойства, которые жест изменил, — по порядку `position`, `size`, `height`, `rotation`; `previous` —
 * записанное в файле до жеста, `undefined`, если свойства не было (отмена тогда его убирает).
 * Высота и поворот, которых не было в файле, считаются нетронутыми, пока равны умолчанию.
 */
export function diffPlacements(start: ObjectPlacement, end: ObjectPlacement): PlacementChange[] {
  const changes: PlacementChange[] = [];
  if (!isSameVec2(start.position, end.position)) {
    changes.push({ key: "position", value: [end.position[0], end.position[1]], previous: [start.position[0], start.position[1]] });
  }
  if (!isSameVec2(start.size, end.size)) {
    changes.push({ key: "size", value: [end.size[0], end.size[1]], previous: [start.size[0], start.size[1]] });
  }
  const startHeight = effectiveHeight(start);
  const endHeight = effectiveHeight(end);
  if (endHeight !== null && endHeight !== startHeight) {
    changes.push({ key: "height", value: endHeight, previous: start.height ?? undefined });
  }
  const startRotation = normalizeRotation(start.rotation ?? 0);
  if (end.rotation !== null && normalizeRotation(end.rotation) !== startRotation) {
    changes.push({ key: "rotation", value: normalizeRotation(end.rotation), previous: start.rotation ?? undefined });
  }
  return changes;
}
