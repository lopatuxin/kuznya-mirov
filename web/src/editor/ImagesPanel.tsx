import { useEffect, useRef } from "react";
import { IMAGE_DRAG_TYPE } from "./imageDrag";
import { drawThumbnail, THUMBNAIL_SIZE_PX } from "./imageThumbnail";
import { imageFrameSize, type ProjectImageTile } from "./projectImages";

type ImagesPanelProps = { tiles: readonly ProjectImageTile[] };

function ImageThumbnail({ tile }: { tile: ProjectImageTile }): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { image, description } = tile;

  // Уменьшенный кадр рисуется один раз на загрузку проекта: пока картинка та же, перерисовки нет.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas && image) drawThumbnail(canvas, image, imageFrameSize(image, description), description.smooth);
  }, [image, description]);

  if (image === null) return <div className="images-panel__thumb images-panel__thumb--empty" />;
  const className = description.smooth ? "images-panel__thumb" : "images-panel__thumb images-panel__thumb--pixelated";
  return <canvas ref={canvasRef} className={className} width={THUMBNAIL_SIZE_PX} height={THUMBNAIL_SIZE_PX} />;
}

/**
 * Вкладка «Картинки» — «Редактор», «Окно редактора», требование 24: картинки `files.images` в порядке объявления,
 * у каждой уменьшенный первый кадр и имя; картинку перетаскивают на плоскую сцену. Не прочитанную и не разжатую
 * картинку не перетащить.
 */
export function ImagesPanel({ tiles }: ImagesPanelProps): React.JSX.Element {
  if (tiles.length === 0) {
    return <div className="problems-panel problems-panel--empty">В игре нет картинок</div>;
  }
  return (
    <ul className="images-panel__list">
      {tiles.map((tile) => {
        const name = tile.description.name;
        return (
          <li
            key={name}
            className={tile.image === null ? "images-panel__item images-panel__item--unreadable" : "images-panel__item"}
            draggable={tile.image !== null}
            onDragStart={(event) => {
              event.dataTransfer.setData(IMAGE_DRAG_TYPE, name);
              event.dataTransfer.effectAllowed = "copy";
            }}
            title={tile.image === null ? "Файл картинки не прочитан" : "Перетащите на сцену"}
          >
            <ImageThumbnail tile={tile} />
            <span className="images-panel__name">{name}</span>
          </li>
        );
      })}
    </ul>
  );
}
