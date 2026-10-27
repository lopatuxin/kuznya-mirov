import { useSyncExternalStore } from "react";
import { effectiveBoneTurn, isMarked, markIndexAt, motionsTextWithBoneTurn, motionsTextWithMark, motionsTextWithoutBones, motionsTextWithoutMark, type BoneCorrection, type BoneTurn } from "./motionsFile";
import { StandSlider } from "./StandSlider";
import type { StandPlayback } from "./standPlayback";
import type { AnimationStand } from "./useAnimationStand";

const AXES = ["X", "Y", "Z"] as const;

/** Доля движения, а у кости без отметок — всегда 0: постоянная поправка от доли не зависит, и играющее движение её не перерисовывает. */
function useBonePhase(playback: StandPlayback, isMarkedBone: boolean): number {
  return useSyncExternalStore(playback.subscribe, () => (isMarkedBone ? Math.round(playback.getState().phase * 1000) / 1000 : 0));
}

function turnWith(turn: BoneTurn, axis: number, value: number): BoneTurn {
  return [axis === 0 ? value : turn[0], axis === 1 ? value : turn[1], axis === 2 ? value : turn[2]];
}

type StandBoneControlsProps = { stand: AnimationStand; boneName: string; isEditable: boolean };

/** Выбранная кость — «Анимация», требование 9: три ползунка поворота, «Отметка» и сброс. */
export function StandBoneControls({ stand, boneName, isEditable }: StandBoneControlsProps): React.JSX.Element {
  const { playback, isCyclic, corrections } = stand;
  const correction: BoneCorrection | undefined = corrections[boneName];
  const marks = isMarked(correction) ? correction : null;
  const phase = useBonePhase(playback, marks !== null);
  const turn = effectiveBoneTurn(correction, phase, isCyclic);
  const markIndex = marks === null ? -1 : markIndexAt(marks, phase);

  function pauseWhenMarked(isActive: boolean): void {
    stand.setGestureActive(isActive);
    if (isActive && marks !== null) playback.setPlaying(false);
  }

  function turnChange(axis: number, value: number): (text: string | null, name: string) => string | null {
    return (text, name) => motionsTextWithBoneTurn(text, name, boneName, playback.getState().phase, turnWith(turn, axis, value));
  }

  return (
    <div className="stand-bone">
      {AXES.map((label, axis) => (
        <StandSlider
          key={label}
          label={`Поворот ${label}`}
          value={turn[axis] as number}
          min={-180}
          max={180}
          step={1}
          unit="°"
          isDisabled={!isEditable}
          onPreview={(value) => stand.preview(turnChange(axis, value))}
          onCommit={(value) => stand.commit(turnChange(axis, value))}
          onRevert={() => stand.commit(() => null)}
          onGestureActiveChange={pauseWhenMarked}
        />
      ))}
      <div className="stand-bone__buttons">
        <button
          type="button"
          className="editor-button editor-button--outline"
          disabled={!isEditable}
          title={markIndex >= 0 ? "Убрать отметку в этой доле движения" : "Поставить отметку: поправка кости начнёт меняться по долям движения"}
          onClick={() =>
            stand.commit((text, name) =>
              markIndex >= 0 ? motionsTextWithoutMark(text, name, boneName, markIndex) : motionsTextWithMark(text, name, boneName, playback.getState().phase, isCyclic),
            )
          }
        >
          {markIndex >= 0 ? "Убрать отметку" : "Отметка"}
        </button>
        <button type="button" className="editor-button" disabled={!isEditable || correction === undefined} onClick={() => stand.commit((text, name) => motionsTextWithoutBones(text, name, boneName))}>
          Сбросить кость
        </button>
      </div>
    </div>
  );
}
