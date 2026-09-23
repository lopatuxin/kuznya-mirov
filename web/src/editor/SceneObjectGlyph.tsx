import { EditorIcon } from "./EditorIcon";
import type { SceneObjectSummary } from "./sceneObjects";

type SceneObjectGlyphProps = { object: SceneObjectSummary; isLarge?: boolean };

/**
 * Значок объекта в списке и в шапке свойств: образец цвета, значок картинки, пунктирная рамка у
 * невидимого объекта на сцене и перечёркнутый глаз у объекта, которого на сцене нет.
 */
export function SceneObjectGlyph({ object, isLarge = false }: SceneObjectGlyphProps): React.JSX.Element {
  const sizeClass = isLarge ? " scene-object-glyph--large" : "";
  const iconSize = isLarge ? 16 : 13;

  if (!object.isOnScene) {
    return (
      <span className={`scene-object-glyph scene-object-glyph--off-scene${sizeClass}`}>
        <EditorIcon name="off-scene" size={iconSize} />
      </span>
    );
  }
  if (object.color !== null) {
    return <span className={`scene-object-glyph scene-object-glyph--color${sizeClass}`} style={{ background: object.color }} />;
  }
  if (object.image !== null) {
    return (
      <span className={`scene-object-glyph scene-object-glyph--image${sizeClass}`}>
        <EditorIcon name="image" size={iconSize} />
      </span>
    );
  }
  return <span className={`scene-object-glyph scene-object-glyph--invisible${sizeClass}`} />;
}
