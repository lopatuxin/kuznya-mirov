export type SceneConfig = {
  width: number;
  height: number;
};

const FALLBACK_SCENE: SceneConfig = { width: 32, height: 24 };

type RawGameJson = {
  scene?: {
    width?: unknown;
    height?: unknown;
  };
};

function isPositiveFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

/**
 * Читает `scene.width`/`height` прямо из текста `game.json`, минуя движок: страница сама вписывает
 * холст в окно по этому соотношению сторон, ещё до того как движок создан и провёл полную
 * предстартовую проверку. Итоговую проверку самого файла всё равно делает движок в `read_entry` —
 * здесь достаточно разумного запасного размера, если файл ещё не разобрать. Ноль, отрицательное
 * значение и NaN отбрасываются наравне с отсутствующим полем: `computeCanvasLayout` вызывается этим
 * размером ещё до `read_entry`, и вырожденный холст (например 1×1) человек увидел бы раньше текста
 * ошибки.
 */
export function parseSceneConfig(gameJsonText: string): SceneConfig {
  try {
    const parsed = JSON.parse(gameJsonText) as RawGameJson;
    const scene = parsed.scene;
    if (scene && isPositiveFiniteNumber(scene.width) && isPositiveFiniteNumber(scene.height)) {
      return { width: scene.width, height: scene.height };
    }
  } catch {
    // game.json недействителен — итоговую ошибку с местом и сутью проблемы выдаст read_entry.
  }
  return FALLBACK_SCENE;
}
