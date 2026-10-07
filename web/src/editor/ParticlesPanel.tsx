import { useId, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { ImageThumbnail } from "./ImagesPanel";
import { PARTICLES_DRAG_TYPE } from "./imageDrag";
import { ParticleKindFields } from "./ParticleKindFields";
import { particleKindNameError } from "./particlesEditing";
import { particleKindsWithValue, particleTableOf, type ParticleFields, type ParticleTable } from "./particlesFile";
import type { TabParticleKind } from "./particlePresets";
import type { ProjectImageTile } from "./projectImages";

type ParticlesPanelProps = {
  /** Виды вкладки: готовые дым, искры и листья, дальше свои виды файла («Редактор», требование 25). */
  kinds: readonly TabParticleKind[];
  imageTiles: readonly ProjectImageTile[];
  /** Выбранный вид; не названный или пропавший — выбран первый («Редактор», требование 25). */
  selectedName: string | null;
  /** Виды, у которых проверка нашла ошибку: карточка помечена, поля правятся («Редактор», требование 36). */
  invalidNames: ReadonlySet<string>;
  /** Ошибки проверки загрузкой по видам и полям: текст ошибки стоит у её поля («Редактор», требование 27). */
  loadFieldErrors: ReadonlyMap<string, Readonly<Record<string, string>>>;
  /** Поля вида неактивны: повтор или проект без правки («Редактор», требование 35). */
  isFieldsDisabled: boolean;
  /** Почему «Копировать», «Удалить» и правка имени неактивны (партия, повтор, ошибки); `null` — активны. */
  kindActionsBlockedReason: string | null;
  /** Карточку можно взять и перетащить на сцену. */
  isDragEnabled: boolean;
  onSelect: (name: string) => void;
  /** Ставит таблицу видов на сцене; текст ошибки движка, если он её не принял. */
  onPreview: (table: ParticleTable) => string | undefined;
  /** Поле принято действием: вне партии — запись файла, в партии — живая игра; текст ошибки, если оно не принято. */
  onCommitValue: (name: string, key: string, value: unknown, table: ParticleTable) => string | undefined;
  onCopy: (name: string) => void;
  onDelete: (name: string) => void;
  onRename: (name: string, newName: string) => void;
};

const BUILT_IN_KIND_REASON = "Готовый вид не удаляется и не переименовывается — для своего варианта его копируют";

type KindNameProps = { name: string; kindNames: readonly string[]; isDisabled: boolean; onRename: (newName: string) => void };

/**
 * Имя своего вида: щелчок открывает правку, Enter или уход из поля — действие, как у значения свойства, Esc — отмена
 * («Редактор», требование 32). Enter с именем, которое не принять, правку не закрывает: текст ошибки остаётся под полем.
 */
function KindName({ name, kindNames, isDisabled, onRename }: KindNameProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null);
  // Ушли из поля с именем, которое не принять: правка закрыта, имя прежнее, а ошибка остаётся под ним.
  const [rejection, setRejection] = useState<string | undefined>(undefined);
  const error = draft === null ? undefined : particleKindNameError(draft, name, kindNames);

  function commit(): void {
    if (draft === null) return;
    setDraft(null);
    setRejection(error);
    if (error === undefined && draft !== name) onRename(draft);
  }

  if (draft === null) {
    return (
      <div className="particles-name-editor">
        <button
          type="button"
          className="particles-name"
          disabled={isDisabled}
          title="Щелчок — переименовать вид"
          onClick={() => {
            setRejection(undefined);
            setDraft(name);
          }}
        >
          {name}
        </button>
        {rejection !== undefined && (
          <div className="particles-field__error" role="alert">
            {rejection}
          </div>
        )}
      </div>
    );
  }
  return (
    <div className="particles-name-editor">
      <input
        autoFocus
        type="text"
        aria-label="Имя вида"
        className="particles-name-editor__input"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onFocus={(event) => event.currentTarget.select()}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            if (error === undefined) event.currentTarget.blur();
          } else if (event.key === "Escape") setDraft(null);
        }}
      />
      {error !== undefined && (
        <div className="particles-field__error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}

/** Рисунок карточки — то, чем движок рисует частицы вида: клуб дыма, искра, лист или мягкая точка. */
function ShapeThumbnail({ shape }: { shape: unknown }): React.JSX.Element {
  const id = useId();
  if (shape === "smoke") {
    return (
      <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
        <defs>
          <filter id={`${id}-blur`}>
            <feGaussianBlur stdDeviation="1.6" />
          </filter>
        </defs>
        <g filter={`url(#${id}-blur)`}>
          <circle cx="15" cy="23" r="8" fill="#a9a9b0" />
          <circle cx="25" cy="22" r="8.5" fill="#b4b4bb" />
          <circle cx="20" cy="15" r="8" fill="#d2d2d8" />
          <circle cx="28" cy="14" r="5.5" fill="#dcdce2" />
          <circle cx="12" cy="15" r="5" fill="#c8c8ce" />
        </g>
      </svg>
    );
  }
  if (shape === "spark") {
    return (
      <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
        <defs>
          <radialGradient id={`${id}-glow`}>
            <stop offset="0" stopColor="#fff6d6" />
            <stop offset="0.25" stopColor="#ffd27a" />
            <stop offset="0.55" stopColor="#ff9a3c" stopOpacity="0.6" />
            <stop offset="1" stopColor="#ff7a1a" stopOpacity="0" />
          </radialGradient>
        </defs>
        <circle cx="20" cy="20" r="15" fill={`url(#${id}-glow)`} />
        <circle cx="9" cy="11" r="2.2" fill="#ffc861" />
        <circle cx="31" cy="27" r="1.8" fill="#ffb347" />
      </svg>
    );
  }
  if (shape === "leaf") {
    return (
      <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
        <path d="M8 31 C 9 18, 19 8, 33 8 C 32 21, 22 31, 8 31 Z" fill="#7da33a" />
        <path d="M8 31 C 15 23, 22 16, 31 10" stroke="#c4d77a" strokeWidth="1.3" fill="none" />
        <path d="M24 34 C 25 28, 30 24, 36 24 C 35 30, 30 34, 24 34 Z" fill="#d9a531" />
      </svg>
    );
  }
  return <span className="particles-card__dot" aria-hidden="true" />;
}

function KindThumbnail({ fields, tile }: { fields: ParticleFields; tile: ProjectImageTile | undefined }): React.JSX.Element {
  if (fields.image === undefined) {
    return (
      <div className="particles-card__thumb particles-card__thumb--shape">
        <ShapeThumbnail shape={fields.shape} />
      </div>
    );
  }
  if (tile === undefined) return <div className="particles-card__thumb images-panel__thumb--empty" />;
  return (
    <div className="particles-card__thumb">
      <ImageThumbnail tile={tile} />
    </div>
  );
}

/**
 * Вкладка «Частицы» — «Редактор», «Окно редактора», требование 25: слева сетка карточек — готовые дым, искры и листья,
 * дальше свои виды файла, под ними значки «Копировать» и «Удалить» и подсказка, что карточку тащат на сцену туда, откуда
 * должно идти; справа имя выбранного вида и три колонки его полей. Картинок вкладка не спрашивает: дым, искры и листья
 * рисует движок.
 */
export function ParticlesPanel(props: ParticlesPanelProps): React.JSX.Element | null {
  const { kinds, imageTiles, selectedName, invalidNames, loadFieldErrors, isFieldsDisabled, kindActionsBlockedReason, isDragEnabled, onSelect, onPreview, onCommitValue } = props;
  const selected = kinds.find((kind) => kind.name === selectedName) ?? kinds[0];
  if (selected === undefined) return null;

  const isCopyDisabled = kindActionsBlockedReason !== null;
  const ownKindBlockedReason = kindActionsBlockedReason ?? (selected.isBuiltIn ? BUILT_IN_KIND_REASON : null);
  const tableWith = (key: string, value: unknown): ParticleTable => particleTableOf(particleKindsWithValue(kinds, selected.name, key, value));

  return (
    <div className="particles-panel">
      <div className="particles-panel__kinds">
        <ul className="particles-panel__cards" role="listbox" aria-label="Виды частиц">
          {kinds.map((kind) => {
            const tile = imageTiles.find((candidate) => candidate.description.name === kind.fields.image);
            const className = [
              "particles-card",
              kind.name === selected.name ? "particles-card--selected" : "",
              invalidNames.has(kind.name) ? "particles-card--invalid" : "",
            ]
              .filter((part) => part !== "")
              .join(" ");
            return (
              <li
                key={kind.name}
                role="option"
                aria-selected={kind.name === selected.name}
                className={className}
                draggable={isDragEnabled}
                title={invalidNames.has(kind.name) ? "В виде ошибка проверки — она в «Ошибках»" : isDragEnabled ? "Перетащите на сцену туда, откуда должно идти" : undefined}
                onClick={() => onSelect(kind.name)}
                onDragStart={(event) => {
                  event.dataTransfer.setData(PARTICLES_DRAG_TYPE, kind.name);
                  event.dataTransfer.effectAllowed = "copy";
                }}
              >
                <KindThumbnail fields={kind.fields} tile={tile} />
                <span className="particles-card__name">{kind.name}</span>
              </li>
            );
          })}
        </ul>
        <div className="particles-panel__actions">
          <button type="button" className="editor-button editor-button--icon" aria-label="Копировать" disabled={isCopyDisabled} title={kindActionsBlockedReason ?? "Копировать вид — свой вариант с другими настройками"} onClick={() => props.onCopy(selected.name)}>
            <EditorIcon name="copy" size={14} />
          </button>
          <button type="button" className="editor-button editor-button--icon" aria-label="Удалить" disabled={ownKindBlockedReason !== null} title={ownKindBlockedReason ?? "Удалить вид"} onClick={() => props.onDelete(selected.name)}>
            <EditorIcon name="trash" size={14} />
          </button>
        </div>
        {isDragEnabled && <p className="particles-panel__hint">Перетащите карточку на сцену туда, откуда должно идти — например, на трубу избы</p>}
      </div>
      <ParticleKindFields
        key={selected.name}
        kind={selected}
        title={
          selected.isBuiltIn ? (
            <span className="particles-name particles-name--fixed">{selected.name}</span>
          ) : (
            <KindName
              name={selected.name}
              kindNames={kinds.map((kind) => kind.name)}
              isDisabled={ownKindBlockedReason !== null}
              onRename={(newName) => props.onRename(selected.name, newName)}
            />
          )
        }
        isDisabled={isFieldsDisabled}
        loadErrors={loadFieldErrors.get(selected.name) ?? {}}
        onPreview={(key, value) => onPreview(tableWith(key, value))}
        onCommit={(key, value) => onCommitValue(selected.name, key, value, tableWith(key, value))}
        onRevert={() => onPreview(particleTableOf(kinds))}
      />
    </div>
  );
}
