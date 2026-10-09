import { useEffect, useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { ImageThumbnail } from "./ImagesPanel";
import { FieldError, finishGesture, useHeldValue, type ParticleEditing } from "./ParticleFieldControls";
import type { ProjectImageTile } from "./projectImages";

/** Картинки игры, из которых группа «Облака» собирает список облаков. */
export type CloudImages = {
  /** Все картинки `files.images` с точками для уменьшенных рисунков. */
  tiles: readonly ProjectImageTile[];
  /** Имена тех, что годятся облакам — «Ветер и частицы», «Проверка перед запуском», требование 27. */
  cloudNames: readonly string[];
};

const HINT = "Выберите картинки облаков — без них облаков нет";

function isNameList(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function ImageTileThumbnail({ tile }: { tile: ProjectImageTile | undefined }): React.JSX.Element {
  return tile === undefined ? <div className="images-panel__thumb images-panel__thumb--empty" /> : <ImageThumbnail tile={tile} />;
}

type CloudImagesFieldProps = { editing: ParticleEditing; images: CloudImages };

/**
 * «Картинки облаков» — «Редактор», фаза 34, требование 33: выбранные картинки в ряд с крестиком, кнопка «+ картинка» со списком годных
 * ещё не выбранных, подсказка, пока картинок нет. Добавление и удаление — одна правка списка целиком (требование 34).
 */
export function CloudImagesField({ editing, images }: CloudImagesFieldProps): React.JSX.Element {
  const property = editing.properties.cloud_images;
  const held = useHeldValue<string[]>(isNameList(property) ? property : undefined);
  const [isListOpen, setIsListOpen] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const rootRef = useRef<HTMLDivElement>(null);
  const selected = held.shown ?? [];
  const choices = images.tiles.filter(({ description }) => images.cloudNames.includes(description.name) && !selected.includes(description.name));

  // Список закрывается щелчком мимо него и Esc; Esc до сцены и до игры не доходит, как у окошек верхней полосы.
  useEffect(() => {
    if (!isListOpen) return;
    function closeOutside(event: PointerEvent): void {
      if (!rootRef.current?.contains(event.target as Node)) setIsListOpen(false);
    }
    function closeOnEscape(event: KeyboardEvent): void {
      if (event.code !== "Escape") return;
      event.stopPropagation();
      setIsListOpen(false);
    }
    document.addEventListener("pointerdown", closeOutside, true);
    document.addEventListener("keydown", closeOnEscape, true);
    return () => {
      document.removeEventListener("pointerdown", closeOutside, true);
      document.removeEventListener("keydown", closeOnEscape, true);
    };
  }, [isListOpen]);

  // Список сначала ставится сцене: отказ движка показывается под рядом, а в файл и в живой мир ничего не пишется.
  function change(next: string[]): void {
    const message = editing.onPreview("cloud_images", next);
    setError(message);
    if (message !== undefined) return;
    if (held.move(next)) finishGesture(held, "cloud_images", editing);
  }

  return (
    <div className="particles-field">
      <span className="particles-field__label">картинки облаков</span>
      <div className="cloud-images" ref={rootRef}>
        {selected.length === 0 ? (
          <p className="cloud-images__hint">{HINT}</p>
        ) : (
          <ul className="cloud-images__row" aria-label="Выбранные картинки облаков">
            {selected.map((name, index) => (
              <li key={`${index}-${name}`} className="cloud-images__chip">
                <ImageTileThumbnail tile={images.tiles.find(({ description }) => description.name === name)} />
                <span className="cloud-images__name">{name}</span>
                <button
                  type="button"
                  className="cloud-images__remove"
                  aria-label={`Убрать картинку ${name}`}
                  title="Убрать из списка"
                  disabled={editing.isDisabled}
                  onClick={() => change(selected.filter((_, position) => position !== index))}
                >
                  <EditorIcon name="close" size={12} />
                </button>
              </li>
            ))}
          </ul>
        )}
        <button type="button" className="editor-button cloud-images__add" aria-expanded={isListOpen} disabled={editing.isDisabled} onClick={() => setIsListOpen(!isListOpen)}>
          + картинка
        </button>
        {isListOpen && !editing.isDisabled && (
          <ul className="cloud-images__choices" aria-label="Картинки, годные облакам">
            {choices.length === 0 && <li className="cloud-images__hint">Других подходящих картинок в игре нет</li>}
            {choices.map((tile) => (
              <li key={tile.description.name}>
                <button type="button" className="cloud-images__choice" onClick={() => change([...selected, tile.description.name])}>
                  <ImageTileThumbnail tile={tile} />
                  <span className="cloud-images__name">{tile.description.name}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <FieldError message={error} />
    </div>
  );
}
