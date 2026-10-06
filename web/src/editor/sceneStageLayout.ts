/**
 * Размер холста сцены в CSS-пикселях внутри части окна со сценой, за вычетом полей по краям: холст занимает
 * всю часть окна, а что на нём видно, решает камера — «Редактор», требование 42.
 */
export function fitSceneStage(areaWidth: number, areaHeight: number, padding: number): { width: number; height: number } {
  return { width: Math.floor(Math.max(1, areaWidth - padding * 2)), height: Math.floor(Math.max(1, areaHeight - padding * 2)) };
}
