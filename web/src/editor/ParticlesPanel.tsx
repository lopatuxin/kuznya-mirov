import { useEffect, useRef } from "react";
import { LEAVES_DRAG_TYPE, PARTICLES_DRAG_TYPE } from "./imageDrag";
import { effectsOfObject, PARTICLE_EFFECTS } from "./particleEffects";
import { ParticleCardArt } from "./ParticleCardArt";
import { ParticleEffectGroup } from "./ParticleEffectFields";
import type { ParticleEditing } from "./ParticleFieldControls";

type ParticlesPanelProps = {
  /** Свойства выбранного объекта: из текста сцены вне партии, из живого мира в ней; `null` — объект не выбран. */
  properties: Readonly<Record<string, unknown>> | null;
  /** Поля и карточки неактивны: повтор или проект без правки. */
  isEditable: boolean;
  onPreview: ParticleEditing["onPreview"];
  onCommit: ParticleEditing["onCommit"];
  onRemove: ParticleEditing["onRemove"];
  onGestureActiveChange: ParticleEditing["onGestureActiveChange"];
};

const EMPTY_HINT = "Дым и искры перетащите туда, откуда они идут, — на трубу или костёр. Листья — на дерево";

/**
 * Вкладка «Частицы» — «Редактор», «Окно редактора», требования 28–32: слева три карточки с рисунками движка — дым, искры и
 * листья, их тащат на сцену; справа группы эффектов выбранного объекта, а у объекта без частиц и без выбора — подсказка.
 */
export function ParticlesPanel({ properties, isEditable, onPreview, onCommit, onRemove, onGestureActiveChange }: ParticlesPanelProps): React.JSX.Element {
  const effects = effectsOfObject(properties);
  const isGestureActiveRef = useRef(false);
  const gestureChangeRef = useRef(onGestureActiveChange);
  gestureChangeRef.current = onGestureActiveChange;
  const handleGestureActiveChange = (isActive: boolean): void => {
    isGestureActiveRef.current = isActive;
    onGestureActiveChange(isActive);
  };
  // Панель пересоздаётся при смене объекта и партии: жест, который не дошёл до отпускания, не должен оставить перезагрузку закрытой.
  useEffect(
    () => () => {
      if (isGestureActiveRef.current) gestureChangeRef.current(false);
    },
    [],
  );
  const editing: ParticleEditing | null = properties === null ? null : { properties, isDisabled: !isEditable, onPreview, onCommit, onRemove, onGestureActiveChange: handleGestureActiveChange };

  return (
    <div className="particles-panel">
      <div className="particles-panel__cards-area">
        <ul className="particles-panel__cards" aria-label="Эффекты частиц">
          {PARTICLE_EFFECTS.map((effect) => (
            <li
              key={effect.id}
              className="particles-card"
              draggable={isEditable}
              title={isEditable ? "Перетащите на сцену" : undefined}
              onDragStart={(event) => {
                event.dataTransfer.setData(PARTICLES_DRAG_TYPE, effect.id);
                if (effect.id === "leaves") event.dataTransfer.setData(LEAVES_DRAG_TYPE, effect.id);
                event.dataTransfer.effectAllowed = "copy";
              }}
            >
              <div className="particles-card__thumb">
                <ParticleCardArt effectId={effect.id} />
              </div>
              <span className="particles-card__name">{effect.cardLabel}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="particles-fields">
        {editing === null || effects.length === 0 ? (
          <p className="particles-panel__hint">{EMPTY_HINT}</p>
        ) : (
          <div className="particles-groups">
            {effects.map((effect) => (
              <ParticleEffectGroup key={effect.id} effect={effect} editing={editing} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
