import { useEffect, useRef, useState } from "react";
import { ParticleNumberInput } from "./ParticleNumberInput";
import { parseNumberField } from "./terrainBrush";

type StandSliderProps = {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  /** Подпись справа от поля: «°», «×». */
  unit?: string;
  isDisabled?: boolean;
  /** Движение ползунка или набранное число: показать на стенде, не записывая. */
  onPreview: (value: number) => void;
  /** Ползунок отпущен или число принято, и оно другое: записать. */
  onCommit: (value: number) => void;
  /** Жест кончился тем же значением, с какого начался: писать нечего, стенд возвращается к файлу. */
  onRevert: () => void;
  /** Жест начался и кончился: пока он идёт, перезагрузка файлов проекта ждёт. */
  onGestureActiveChange: (isActive: boolean) => void;
};

function formatValue(value: number): string {
  return String(Math.round(value * 100) / 100);
}

/** Ползунок с полем числа — «Анимация», требования 8–9: движение ползунка меняет модель сразу, запись — при отпускании. */
export function StandSlider({ label, value, min, max, step, unit, isDisabled = false, onPreview, onCommit, onRevert, onGestureActiveChange }: StandSliderProps): React.JSX.Element {
  const [draft, setDraft] = useState<number | null>(null);
  const [text, setText] = useState<string | null>(null);
  const rangeRef = useRef<HTMLInputElement>(null);
  const callbacksRef = useRef({ onCommit, onRevert, onGestureActiveChange, value });
  callbacksRef.current = { onCommit, onRevert, onGestureActiveChange, value };

  // Принятое значение дошло до файла: ползунок показывает уже его, а не черновик жеста.
  useEffect(() => setDraft(null), [value]);

  // Отпускание ползунка — это `change`, а не `input`, который идёт на каждое движение.
  useEffect(() => {
    const range = rangeRef.current;
    if (range === null) return;
    function handleChange(): void {
      const committed = Number(range?.value);
      callbacksRef.current.onGestureActiveChange(false);
      if (committed === callbacksRef.current.value) {
        setDraft(null);
        callbacksRef.current.onRevert();
      } else {
        callbacksRef.current.onCommit(committed);
      }
    }
    range.addEventListener("change", handleChange);
    return () => range.removeEventListener("change", handleChange);
  }, []);

  function finishText(): void {
    if (text === null) return;
    const parsed = parseNumberField(text);
    setText(null);
    onGestureActiveChange(false);
    if (parsed === null || parsed === value) onRevert();
    else onCommit(parsed);
  }

  return (
    <div className="stand-slider">
      <span className="stand-slider__label">{label}</span>
      <input
        ref={rangeRef}
        type="range"
        aria-label={label}
        className="stand-slider__range"
        min={min}
        max={max}
        step={step}
        value={draft ?? Math.min(max, Math.max(min, value))}
        disabled={isDisabled}
        onChange={(event) => {
          const next = Number(event.target.value);
          if (draft === null) onGestureActiveChange(true);
          setDraft(next);
          onPreview(next);
        }}
      />
      <span className="stand-slider__field">
        <ParticleNumberInput
          label={`${label}, число`}
          text={text ?? formatValue(draft ?? value)}
          isDefault={false}
          isDisabled={isDisabled}
          onTextChange={(next) => {
            if (text === null) onGestureActiveChange(true);
            setText(next);
            const parsed = parseNumberField(next);
            if (parsed !== null) onPreview(parsed);
          }}
          onEnter={finishText}
          onBlur={finishText}
          onEscape={() => {
            setText(null);
            onGestureActiveChange(false);
            onRevert();
          }}
        />
        {unit !== undefined && <span className="stand-slider__unit">{unit}</span>}
      </span>
    </div>
  );
}
