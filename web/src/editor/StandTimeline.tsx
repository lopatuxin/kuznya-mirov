import { useEffect, useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { isMarked, motionsTextWithMovedMark, motionsTextWithoutMark, type TurnMark } from "./motionsFile";
import { useStandPlayback } from "./standPlayback";
import type { AnimationStand, StandSlip } from "./useAnimationStand";

/** Засечка сдвинута меньше этого — это щелчок, а не перетаскивание: доля встаёт на неё, файл не пишется. */
const MARK_DRAG_THRESHOLD = 0.004;
const TOLERANCE = 0.003;

type StandTimelineProps = {
  stand: AnimationStand;
  /** Имя выбранной кости: её отметки видны на шкале засечками. */
  boneName: string | undefined;
  isEditable: boolean;
};

type MarkDrag = { index: number; startPhase: number; lowest: number; highest: number; isMoved: boolean };

function percent(share: number): string {
  return `${share * 100}%`;
}

function slipSpans(slip: StandSlip): { from: number; to: number }[] {
  return slip.to >= slip.from
    ? [{ from: slip.from, to: slip.to }]
    : [
        { from: slip.from, to: 1 },
        { from: 0, to: slip.to },
      ];
}

function describeSlip(slip: StandSlip): string {
  return `Ступня ${slip.foot + 1} скользит по земле: доли ${slip.from.toFixed(2)}–${slip.to.toFixed(2)}`;
}

/** Шкала под просмотром — «Анимация», требования 7, 9, 12: пуск, пауза, шаг по кадру, перетаскивание доли, засечки отметок и красные метки. */
export function StandTimeline({ stand, boneName, isEditable }: StandTimelineProps): React.JSX.Element {
  const { playback, info, analysis, corrections } = stand;
  const { phase, isPlaying } = useStandPlayback(playback);
  const trackRef = useRef<HTMLDivElement>(null);
  const isScrubbingRef = useRef(false);
  const markDragRef = useRef<MarkDrag | null>(null);
  // Засечка идёт за мышью, пока принятая правка не дошла до файла.
  const [draggedMark, setDraggedMark] = useState<{ index: number; at: number } | null>(null);
  const correction = boneName === undefined ? undefined : corrections[boneName];
  const marks: readonly TurnMark[] = isMarked(correction) ? correction : [];
  const length = info?.length ?? 0;
  const frames = info?.frames ?? 1;

  useEffect(() => setDraggedMark(null), [correction]);

  function phaseAt(clientX: number): number {
    const bounds = trackRef.current?.getBoundingClientRect();
    if (bounds === undefined || bounds.width === 0) return 0;
    return Math.min(1, Math.max(0, (clientX - bounds.left) / bounds.width));
  }

  function moveMark(index: number, share: number): void {
    if (boneName === undefined) return;
    stand.previewNow((text, name) => motionsTextWithMovedMark(text, name, boneName, index, share));
  }

  function handleMarkDown(event: React.PointerEvent<HTMLButtonElement>, index: number): void {
    if (event.button !== 0) return;
    event.stopPropagation();
    const mark = marks[index];
    if (mark === undefined) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    playback.setPlaying(false);
    playback.setPhase(mark.at);
    markDragRef.current = {
      index,
      startPhase: mark.at,
      lowest: (marks[index - 1]?.at ?? -1) + TOLERANCE,
      highest: (marks[index + 1]?.at ?? 2) - TOLERANCE,
      isMoved: false,
    };
  }

  function handleMarkMove(event: React.PointerEvent<HTMLButtonElement>): void {
    const drag = markDragRef.current;
    if (drag === null || !isEditable) return;
    const share = Math.min(drag.highest, Math.max(drag.lowest, phaseAt(event.clientX), 0), 1);
    if (!drag.isMoved && Math.abs(share - drag.startPhase) < MARK_DRAG_THRESHOLD) return;
    if (!drag.isMoved) stand.setGestureActive(true);
    drag.isMoved = true;
    setDraggedMark({ index: drag.index, at: share });
    playback.setPhase(share);
    moveMark(drag.index, share);
  }

  function handleMarkUp(event: React.PointerEvent<HTMLButtonElement>): void {
    const drag = markDragRef.current;
    markDragRef.current = null;
    if (drag === null || !drag.isMoved || boneName === undefined) {
      setDraggedMark(null);
      return;
    }
    const share = playback.getState().phase;
    stand.setGestureActive(false);
    stand.commit((text, name) => motionsTextWithMovedMark(text, name, boneName, drag.index, share));
    event.currentTarget.releasePointerCapture?.(event.pointerId);
  }

  function removeMark(index: number): void {
    if (boneName === undefined || !isEditable) return;
    stand.commit((text, name) => motionsTextWithoutMark(text, name, boneName, index));
  }

  return (
    <div className="stand-timeline">
      <div className="stand-timeline__buttons">
        <button
          type="button"
          className="editor-button editor-button--icon"
          title={isPlaying ? "Остановить движение" : "Играть движение"}
          aria-label={isPlaying ? "Остановить движение" : "Играть движение"}
          onClick={() => playback.setPlaying(!isPlaying)}
        >
          <EditorIcon name={isPlaying ? "pause" : "play"} size={15} />
        </button>
        <button type="button" className="editor-button editor-button--icon" title="Кадр назад" aria-label="Кадр назад" onClick={() => playback.step(-1, frames)}>
          <EditorIcon name="step-back" size={14} />
        </button>
        <button type="button" className="editor-button editor-button--icon" title="Кадр вперёд" aria-label="Кадр вперёд" onClick={() => playback.step(1, frames)}>
          <EditorIcon name="step-forward" size={14} />
        </button>
      </div>
      <div
        ref={trackRef}
        className="stand-timeline__track"
        role="slider"
        aria-label="Доля движения"
        aria-valuemin={0}
        aria-valuemax={1}
        aria-valuenow={Math.round(phase * 1000) / 1000}
        tabIndex={0}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          isScrubbingRef.current = true;
          playback.setPlaying(false);
          playback.setPhase(phaseAt(event.clientX));
        }}
        onPointerMove={(event) => {
          if (isScrubbingRef.current) playback.setPhase(phaseAt(event.clientX));
        }}
        onPointerUp={() => {
          isScrubbingRef.current = false;
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowLeft") playback.step(-1, frames);
          else if (event.key === "ArrowRight") playback.step(1, frames);
          else return;
          event.preventDefault();
        }}
      >
        {analysis.slips.flatMap((slip) =>
          slipSpans(slip).map((span) => (
            <span
              key={`${slip.foot}-${span.from}`}
              className="stand-timeline__slip"
              style={{ left: percent(span.from), width: percent(span.to - span.from), top: slip.foot === 0 ? 0 : "50%" }}
              title={describeSlip(slip)}
            />
          )),
        )}
        {analysis.endGap !== null && <span className="stand-timeline__gap" title={`Конец расходится с началом на ${Math.round(analysis.endGap.degrees)}° (${analysis.endGap.bone})`} />}
        {marks.map((mark, index) => (
          <button
            key={index}
            type="button"
            className="stand-timeline__mark"
            style={{ left: percent(draggedMark?.index === index ? draggedMark.at : mark.at) }}
            title={`Отметка ${mark.at.toFixed(3)}: перетащить по шкале, двойной щелчок или Delete — убрать`}
            aria-label={`Отметка ${mark.at.toFixed(3)}`}
            onPointerDown={(event) => handleMarkDown(event, index)}
            onPointerMove={handleMarkMove}
            onPointerUp={handleMarkUp}
            onPointerCancel={handleMarkUp}
            onDoubleClick={() => removeMark(index)}
            onKeyDown={(event) => {
              if (event.key !== "Delete" && event.key !== "Backspace") return;
              event.preventDefault();
              removeMark(index);
            }}
          />
        ))}
        <span className="stand-timeline__head" style={{ left: percent(phase) }} />
      </div>
      <div className="stand-timeline__readout">
        <span>доля {phase.toFixed(3)}</span>
        <span>{info === null ? "—" : `${(phase * length).toFixed(2)} с из ${length.toFixed(2)} с`}</span>
        <span>{info === null ? "—" : `${frames} кадров`}</span>
        {analysis.endGap !== null && (
          <span className="stand-timeline__warning">
            конец расходится с началом на {Math.round(analysis.endGap.degrees)}° ({analysis.endGap.bone})
          </span>
        )}
        {analysis.slips.length > 0 && <span className="stand-timeline__warning">ступня скользит: {analysis.slips.length}</span>}
      </div>
    </div>
  );
}
