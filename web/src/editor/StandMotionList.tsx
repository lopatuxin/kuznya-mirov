import { useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { readMotionSettings } from "./motionsFile";
import type { ProjectModelRig } from "../projectLoader";

type StandMotionListProps = {
  motionsText: string | null;
  motionNames: readonly string[];
  selectedName: string | null;
  /** Модели игры со скелетом: из них выбирается модель нового движения. */
  rigs: readonly ProjectModelRig[];
  /** Текст файлов проекта не показан (ошибки в `game.json`) — новых движений не создать. */
  canEdit: boolean;
  onSelect: (name: string) => void;
  onCreate: (name: string, model: string, clip: string) => void;
};

/** Почему «Создать» недоступно; `null` — можно. Имя движения не должно совпадать ни с другим движением `motions.json`, ни с движением файла модели. */
export function newMotionProblem(name: string, motionNames: readonly string[], rig: ProjectModelRig | undefined, clip: string): string | null {
  if (rig === undefined || clip === "") return "Выберите модель и движение файла";
  if (name.trim() === "") return "Введите имя нового движения";
  if (motionNames.includes(name)) return "Это имя уже занято другим движением";
  if (rig.clips.includes(name)) return "Это имя занято движением файла модели";
  return null;
}

/** Имя, которое предлагает форма: модель и движение файла, а занятое получает номер. */
export function suggestedMotionName(model: string, clip: string, taken: readonly string[]): string {
  const base = `${model}_${clip.toLowerCase()}`;
  for (let attempt = 1; ; attempt += 1) {
    const name = attempt === 1 ? base : `${base}_${attempt}`;
    if (!taken.includes(name)) return name;
  }
}

type NewMotionFormProps = Pick<StandMotionListProps, "motionNames" | "rigs" | "onCreate"> & { onDone: () => void };

function NewMotionForm({ motionNames, rigs, onCreate, onDone }: NewMotionFormProps): React.JSX.Element {
  const [model, setModel] = useState(rigs[0]?.name ?? "");
  const rig = rigs.find((candidate) => candidate.name === model);
  const [clip, setClip] = useState(rig?.clips[0] ?? "");
  const [typedName, setTypedName] = useState<string | null>(null);
  const name = typedName ?? suggestedMotionName(model, clip, [...motionNames, ...(rig?.clips ?? [])]);
  const problem = newMotionProblem(name, motionNames, rig, clip);

  return (
    <form
      className="stand-new-motion"
      onSubmit={(event) => {
        event.preventDefault();
        if (problem !== null) return;
        onCreate(name, model, clip);
        onDone();
      }}
    >
      <div className="stand-new-motion__title">Новое движение</div>
      <label className="stand-new-motion__field">
        <span>Модель</span>
        <select
          value={model}
          onChange={(event) => {
            const next = rigs.find((candidate) => candidate.name === event.target.value);
            setModel(event.target.value);
            setClip(next?.clips[0] ?? "");
          }}
        >
          {rigs.map((candidate) => (
            <option key={candidate.name} value={candidate.name}>
              {candidate.name}
            </option>
          ))}
        </select>
      </label>
      <label className="stand-new-motion__field">
        <span>Движение файла</span>
        <select value={clip} onChange={(event) => setClip(event.target.value)}>
          {(rig?.clips ?? []).map((candidate) => (
            <option key={candidate} value={candidate}>
              {candidate}
            </option>
          ))}
        </select>
      </label>
      <label className="stand-new-motion__field">
        <span>Имя</span>
        <input type="text" value={name} onChange={(event) => setTypedName(event.target.value)} />
      </label>
      {problem !== null && <p className="stand-new-motion__problem">{problem}</p>}
      <div className="stand-new-motion__buttons">
        <button type="submit" className="editor-button editor-button--outline" disabled={problem !== null}>
          Создать
        </button>
        <button type="button" className="editor-button" onClick={onDone}>
          Отмена
        </button>
      </div>
    </form>
  );
}

/** Левая колонка экрана «Анимация» — требование 5: движения `motions.json` и «Новое движение». */
export function StandMotionList({ motionsText, motionNames, selectedName, rigs, canEdit, onSelect, onCreate }: StandMotionListProps): React.JSX.Element {
  const [isFormOpen, setFormOpen] = useState(false);
  const isCreationPossible = canEdit && rigs.length > 0;
  const isFormShown = isCreationPossible && (isFormOpen || motionNames.length === 0);

  return (
    <div className="stand-list">
      <div className="editor-panel-header">
        Движения
        <span className="editor-count">{motionNames.length}</span>
      </div>
      <div className="stand-list__body">
        {motionNames.length === 0 && <p className="stand-list__empty">В motions.json пока нет движений. Новое движение заводит файл и ключ files.motions в game.json.</p>}
        <ul className="stand-list__items">
          {motionNames.map((name) => {
            const settings = readMotionSettings(motionsText, name);
            return (
              <li key={name}>
                <button type="button" className={name === selectedName ? "stand-list__item stand-list__item--selected" : "stand-list__item"} onClick={() => onSelect(name)}>
                  <span className="stand-list__name">{name}</span>
                  {settings !== null && (
                    <span className="stand-list__source">
                      {settings.model} · {settings.clip}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
        {isFormShown ? (
          <NewMotionForm motionNames={motionNames} rigs={rigs} onCreate={onCreate} onDone={() => setFormOpen(false)} />
        ) : (
          <button
            type="button"
            className="editor-button editor-button--outline stand-list__new"
            disabled={!isCreationPossible}
            title={isCreationPossible ? undefined : "В игре нет моделей со скелетом"}
            onClick={() => setFormOpen(true)}
          >
            <EditorIcon name="plus" size={14} />
            Новое движение
          </button>
        )}
      </div>
    </div>
  );
}
