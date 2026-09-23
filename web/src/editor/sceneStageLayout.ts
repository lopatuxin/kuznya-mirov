import type { SceneSize } from "./sceneObjects";

/**
 * Размер холста сцены в CSS-пикселях внутри части окна со сценой, за вычетом полей по краям.
 * Размер сцены известен — холст берёт её пропорции, и сцена выглядит листом на подложке, а не
 * полосами фона по бокам. Неизвестен — холст занимает всё место, сцену внутрь вписывает движок.
 */
export function fitSceneStage(
  areaWidth: number,
  areaHeight: number,
  sceneSize: SceneSize | null,
  padding: number,
): { width: number; height: number } {
  const innerWidth = Math.max(1, areaWidth - padding * 2);
  const innerHeight = Math.max(1, areaHeight - padding * 2);
  if (sceneSize === null) return { width: Math.floor(innerWidth), height: Math.floor(innerHeight) };
  const scale = Math.min(innerWidth / sceneSize.width, innerHeight / sceneSize.height);
  return {
    width: Math.max(1, Math.round(sceneSize.width * scale)),
    height: Math.max(1, Math.round(sceneSize.height * scale)),
  };
}
