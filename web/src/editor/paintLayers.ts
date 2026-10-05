import type { TerrainCoverLayer } from "./terrainFile";

/** Слоёв покрытий не больше восьми — «Свет и материалы», проверки перед запуском. */
export const MAX_COVER_LAYERS = 8;

/** Номер слоя материала — верхнего из слоёв с ним («Покраска», требование 8); `-1` — материала среди слоёв нет. */
export function topLayerIndex(covers: readonly TerrainCoverLayer[], material: string): number {
  for (let index = covers.length - 1; index >= 0; index -= 1) {
    if (covers[index]?.material === material) return index;
  }
  return -1;
}

/** Материалом можно красить: он уже среди слоёв или слоёв меньше восьми — «Покраска», требования 3, 8. */
export function canPaintMaterial(covers: readonly TerrainCoverLayer[] | null, material: string): boolean {
  return covers === null || covers.length < MAX_COVER_LAYERS || topLayerIndex(covers, material) >= 0;
}

/**
 * Материал кисти: выбранный, пока он объявлен и им можно красить, иначе первый, которым можно, — «Покраска», требования 3, 8.
 * Слоёв восемь, а первого материала среди них нет — кнопка «Материалы» не обещает красить тем, чем мазок не пойдёт.
 */
export function resolvePaintMaterial(materialNames: readonly string[], chosen: string | null, blocked: ReadonlySet<string>): string | undefined {
  const isPaintable = (name: string): boolean => !blocked.has(name);
  return materialNames.find((name) => name === chosen && isPaintable(name)) ?? materialNames.find(isPaintable);
}

/** Путь маски нового слоя — `terrain/<материал>.png`; занят маской другого слоя или картой цвета `tint` — `-2`, `-3` и так далее («Покраска», требование 8). */
export function newMaskPath(covers: readonly TerrainCoverLayer[], material: string, tintPath: string | null): string {
  const taken = new Set(covers.flatMap((layer) => (layer.mask === undefined ? [] : [layer.mask])));
  if (tintPath !== null) taken.add(tintPath);
  for (let attempt = 1; ; attempt += 1) {
    const path = `terrain/${material}${attempt === 1 ? "" : `-${attempt}`}.png`;
    if (!taken.has(path)) return path;
  }
}

/** Слои двух файлов одни и те же; нет покрытий и пустой список — одно и то же. */
export function areCoversEqual(first: readonly TerrainCoverLayer[] | null, second: readonly TerrainCoverLayer[] | null): boolean {
  return JSON.stringify(first ?? []) === JSON.stringify(second ?? []);
}
