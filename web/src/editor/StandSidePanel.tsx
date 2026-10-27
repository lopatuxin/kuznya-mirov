import { useEffect, useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { motionsTextWithSetting, motionsTextWithoutBones, type MotionSettingKey } from "./motionsFile";
import { StandBoneControls } from "./StandBoneControls";
import { boneAncestors, shortBoneName, visibleBoneRows } from "./standBoneTree";
import { StandSlider } from "./StandSlider";
import type { AnimationStand, StandBone } from "./useAnimationStand";

type StandSidePanelProps = {
  stand: AnimationStand;
  /** Движения файла модели выбранного движения: из них выбирается `upper`. */
  fileClips: readonly string[];
  isEditable: boolean;
};

type NumberSetting = {
  key: Extract<MotionSettingKey, "speed" | "bounce" | "lean" | "arms_out">;
  label: string;
  min: number;
  max: number;
  step: number;
  unit: string;
  read: (stand: AnimationStand) => number;
};

const NUMBER_SETTINGS: readonly NumberSetting[] = [
  { key: "speed", label: "скорость", min: 0.1, max: 3, step: 0.05, unit: "×", read: (stand) => stand.settings?.speed ?? 1 },
  { key: "bounce", label: "подскок", min: 0, max: 2, step: 0.05, unit: "×", read: (stand) => stand.settings?.bounce ?? 1 },
  { key: "lean", label: "наклон корпуса", min: -60, max: 60, step: 1, unit: "°", read: (stand) => stand.settings?.lean ?? 0 },
  { key: "arms_out", label: "руки в стороны", min: 0, max: 90, step: 1, unit: "°", read: (stand) => stand.settings?.armsOut ?? 0 },
];

function settingChange(key: MotionSettingKey, value: number | boolean | string | undefined): (text: string | null, name: string) => string | null {
  return (text, name) => motionsTextWithSetting(text, name, key, value);
}

function defaultExpanded(bones: readonly StandBone[]): Set<number> {
  return new Set(bones.flatMap((bone, index) => (bone.parent === -1 ? [index] : [])));
}

type BoneTreeProps = { bones: readonly StandBone[]; selectedBone: number | null; markedBones: ReadonlySet<string>; onSelect: (bone: number) => void };

/** Дерево костей скелета: корни раскрыты, остальное раскрывается стрелкой или выбором кости на модели. */
function BoneTree({ bones, selectedBone, markedBones, onSelect }: BoneTreeProps): React.JSX.Element {
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(() => defaultExpanded(bones));
  const selectedRef = useRef<HTMLButtonElement>(null);

  useEffect(() => setExpanded(defaultExpanded(bones)), [bones]);
  // Кость выбрана щелчком по модели: ветка с ней раскрывается, и кость видна в дереве.
  useEffect(() => {
    if (selectedBone === null) return;
    setExpanded((previous) => new Set([...previous, ...boneAncestors(bones, selectedBone)]));
  }, [bones, selectedBone]);
  useEffect(() => {
    selectedRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [selectedBone, expanded]);

  function toggle(index: number): void {
    setExpanded((previous) => {
      const next = new Set(previous);
      if (!next.delete(index)) next.add(index);
      return next;
    });
  }

  return (
    <ul className="stand-bones" aria-label="Кости">
      {visibleBoneRows(bones, expanded).map((row) => (
        <li key={row.index} className="stand-bones__row" style={{ paddingLeft: 6 + row.depth * 12 }}>
          {row.hasChildren ? (
            <button type="button" className="stand-bones__toggle" aria-label={expanded.has(row.index) ? "Свернуть" : "Развернуть"} onClick={() => toggle(row.index)}>
              <EditorIcon name={expanded.has(row.index) ? "chevron-down" : "arrow-right"} size={11} />
            </button>
          ) : (
            <span className="stand-bones__toggle" />
          )}
          <button
            ref={row.index === selectedBone ? selectedRef : undefined}
            type="button"
            className={row.index === selectedBone ? "stand-bones__bone stand-bones__bone--selected" : "stand-bones__bone"}
            title={row.name}
            onClick={() => onSelect(row.index)}
          >
            {shortBoneName(row.name)}
            {markedBones.has(row.name) && <span className="stand-bones__dot" aria-label="есть поправка" />}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Правая колонка экрана «Анимация» — требования 8–9: общие ручки движения, дерево костей и повороты выбранной кости. */
export function StandSidePanel({ stand, fileClips, isEditable }: StandSidePanelProps): React.JSX.Element {
  const { settings, info, selectedBone, corrections } = stand;
  if (settings === null || stand.selectedName === null) {
    return (
      <div className="stand-side">
        <div className="editor-panel-header">Движение</div>
        <p className="stand-side__empty">Выберите движение слева или создайте новое.</p>
      </div>
    );
  }
  const upperOptions = settings.upper !== null && !fileClips.includes(settings.upper) ? [...fileClips, settings.upper] : fileClips;
  const boneName = selectedBone === null ? undefined : info?.bones[selectedBone]?.name;
  const markedBones = new Set(Object.keys(corrections));

  return (
    <div className="stand-side">
      <div className="editor-panel-header" title={`${settings.model} · ${settings.clip}`}>
        {stand.selectedName}
      </div>
      <div className="stand-side__body">
        <section className="stand-side__section">
          {NUMBER_SETTINGS.map(({ key, label, min, max, step, unit, read }) => (
            <StandSlider
              key={key}
              label={label}
              value={read(stand)}
              min={min}
              max={max}
              step={step}
              unit={unit}
              isDisabled={!isEditable}
              onPreview={(value) => stand.preview(settingChange(key, value))}
              onCommit={(value) => stand.commit(settingChange(key, value))}
              onRevert={() => stand.commit(() => null)}
              onGestureActiveChange={stand.setGestureActive}
            />
          ))}
          <label className="stand-side__row">
            <span className="stand-side__label">верх тела из</span>
            <select value={settings.upper ?? ""} disabled={!isEditable} onChange={(event) => stand.commit(settingChange("upper", event.target.value === "" ? undefined : event.target.value))}>
              <option value="">нет</option>
              {upperOptions.map((clip) => (
                <option key={clip} value={clip}>
                  {clip}
                </option>
              ))}
            </select>
          </label>
          <label className="stand-side__row">
            <span className="stand-side__label">повтор</span>
            <input type="checkbox" checked={stand.isCyclic} disabled={!isEditable} onChange={(event) => stand.commit(settingChange("loop", event.target.checked))} />
          </label>
        </section>
        <section className="stand-side__section">
          <div className="stand-side__heading">
            Кости
            <button type="button" className="editor-button" disabled={!isEditable || markedBones.size === 0} onClick={() => stand.commit((text, name) => motionsTextWithoutBones(text, name))}>
              Сбросить все
            </button>
          </div>
          {info !== null && <BoneTree bones={info.bones} selectedBone={selectedBone} markedBones={markedBones} onSelect={stand.selectBone} />}
          {boneName === undefined ? <p className="stand-side__empty">Выберите кость в дереве или щелчком по модели.</p> : <StandBoneControls stand={stand} boneName={boneName} isEditable={isEditable} />}
        </section>
      </div>
    </div>
  );
}
