import { useEffect, useReducer, useRef, useState } from "react";
import { colorForPicker, DENSITY_STEP, formatParticleNumber, translateParticleKeys, type ParticleEffect } from "./particleEffects";
import { ParticleNumberInput } from "./ParticleNumberInput";
import { ColorPickerInput } from "./PropertiesPanel";
import { parseNumberField } from "./terrainBrush";

/**
 * Что поля вкладки «Частицы» знают о правке выбранного объекта. Ползунок, число и круг ставят значение сцене на лету
 * (`onPreview`), а принятое значение отдают на запись (`onCommit`) — вне партии это `scene.json`, в партии живой мир.
 */
export type ParticleEditing = {
  properties: Readonly<Record<string, unknown>>;
  isDisabled: boolean;
  /** Ставит значение сцене без записи; `undefined` возвращает объекту отсутствие свойства. Текст ошибки движка, если значение не принято. */
  onPreview: (key: string, value: number | undefined) => string | undefined;
  /** Принятое значение; `original` — каким свойство было до жеста (`undefined` — не было). */
  onCommit: (key: string, value: unknown, original: unknown) => void;
  /** «Убрать» и снятие `leaf_color`: свойства снимаются одной правкой. */
  onRemove: (keys: readonly string[]) => void;
  /** Жест ползунка, круга или числа начался (`true`) и кончился (`false`): пока он идёт, перезагрузка файлов проекта ждёт. Повторный вызов с тем же значением безвреден. */
  onGestureActiveChange: (isActive: boolean) => void;
};

function isSameValue(first: unknown, second: unknown): boolean {
  return JSON.stringify(first) === JSON.stringify(second);
}

type Hold<T> = { shown: T; original: T | undefined; isOpen: boolean };

export type HeldValue<T> = {
  /** Что показывать в поле: значение жеста, пока оно не дошло до свойств объекта, иначе само свойство. */
  shown: T | undefined;
  /** Каким свойство было до жеста. */
  original: T | undefined;
  /** Жест меняет значение; ложь — значение то же, что уже показано. */
  move: (next: T) => boolean;
  /** Жест кончился: значение и то, каким свойство было до него; `null` — жеста не было. Показ держится, пока свойство не изменится. */
  settle: () => { shown: T; original: T | undefined } | null;
  /** Жест брошен: поле сразу показывает свойство. */
  drop: () => void;
};

/**
 * Значение поля на время жеста и до тех пор, пока принятая правка не дойдёт до свойств объекта: вне партии запись
 * идёт через очередь, и без этого поле на миг вернуло бы прежнее число.
 */
export function useHeldValue<T>(value: T | undefined): HeldValue<T> {
  const holdRef = useRef<Hold<T> | null>(null);
  const [, forceRender] = useReducer((tick: number) => tick + 1, 0);
  if (holdRef.current !== null && !holdRef.current.isOpen && !isSameValue(value, holdRef.current.original)) holdRef.current = null;
  const hold = holdRef.current;
  return {
    shown: hold === null ? value : hold.shown,
    original: hold === null ? value : hold.original,
    move: (next) => {
      const current = holdRef.current;
      if (current !== null && current.isOpen && isSameValue(current.shown, next)) return false;
      holdRef.current = { shown: next, original: current?.isOpen ? current.original : value, isOpen: true };
      forceRender();
      return true;
    },
    settle: () => {
      const current = holdRef.current;
      if (current === null || !current.isOpen) return null;
      holdRef.current = { ...current, isOpen: false };
      forceRender();
      return { shown: current.shown, original: current.original };
    },
    drop: () => {
      holdRef.current = null;
      forceRender();
    },
  };
}

/** Жест кончился принятым значением: пишется, если оно отличается от прежнего, иначе сцена уже показывает то, что в файле. */
export function finishGesture<T>(held: HeldValue<T>, key: string, editing: ParticleEditing): void {
  const gesture = held.settle();
  if (gesture === null) return;
  if (isSameValue(gesture.shown, gesture.original)) {
    held.drop();
    return;
  }
  editing.onCommit(key, gesture.shown, gesture.original);
}

type FieldErrorProps = { message: string | undefined };

export function FieldError({ message }: FieldErrorProps): React.JSX.Element | null {
  if (message === undefined) return null;
  return (
    <div className="particles-field__error" role="alert">
      {translateParticleKeys(message)}
    </div>
  );
}

type DensityFieldProps = { effect: ParticleEffect; editing: ParticleEditing };

/** «Плотность» — ползунок главного свойства от 0 до 1: на каждое движение сцена меняется сразу, запись — при отпускании. */
export function DensityField({ effect, editing }: DensityFieldProps): React.JSX.Element {
  const value = editing.properties[effect.mainKey];
  const held = useHeldValue<number>(typeof value === "number" ? value : undefined);
  const [error, setError] = useState<string | undefined>(undefined);
  const rangeRef = useRef<HTMLInputElement>(null);
  const finishRef = useRef<() => void>(() => {});
  finishRef.current = () => {
    if (error === undefined) {
      finishGesture(held, effect.mainKey, editing);
    } else {
      held.drop();
      editing.onPreview(effect.mainKey, held.original);
    }
    editing.onGestureActiveChange(false);
  };

  // Отпускание ползунка — это `change`, а не `input`, который идёт на каждое движение.
  useEffect(() => {
    const range = rangeRef.current;
    if (range === null) return;
    const handleChange = (): void => finishRef.current();
    range.addEventListener("change", handleChange);
    return () => range.removeEventListener("change", handleChange);
  }, []);

  return (
    <div className="particles-field">
      <span className="particles-field__label">плотность</span>
      <div className="particles-density">
        <span className="particles-density__end">{effect.densityLabels[0]}</span>
        <input
          ref={rangeRef}
          type="range"
          aria-label="плотность"
          className="particles-density__range"
          min={0}
          max={1}
          step={DENSITY_STEP}
          value={held.shown ?? 0}
          disabled={editing.isDisabled}
          onChange={(event) => {
            const next = Number(event.target.value);
            if (!held.move(next)) return;
            editing.onGestureActiveChange(true);
            setError(editing.onPreview(effect.mainKey, next));
          }}
        />
        <span className="particles-density__end">{effect.densityLabels[1]}</span>
      </div>
      <FieldError message={error} />
    </div>
  );
}

type NumberFieldProps = { label: string; propertyKey: string; defaultValue: number; editing: ParticleEditing };

/** Число настройки: набранное сразу ставится сцене, Enter или уход из поля пишет его, Esc возвращает прежнее. */
export function NumberField({ label, propertyKey, defaultValue, editing }: NumberFieldProps): React.JSX.Element {
  const value = editing.properties[propertyKey];
  const held = useHeldValue<number>(typeof value === "number" ? value : undefined);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | undefined>(undefined);
  const text = draft ?? formatParticleNumber(held.shown ?? defaultValue);

  function revert(): void {
    const original = held.original;
    held.drop();
    editing.onPreview(propertyKey, original);
    editing.onGestureActiveChange(false);
  }

  function change(next: string): void {
    editing.onGestureActiveChange(true);
    setDraft(next);
    const parsed = parseNumberField(next);
    if (parsed === null) {
      setError(undefined);
      return;
    }
    if (held.move(parsed)) setError(editing.onPreview(propertyKey, parsed));
  }

  function finish(): void {
    if (draft === null) return;
    const parsed = parseNumberField(draft);
    if (parsed === null) {
      cancel();
      return;
    }
    const message = editing.onPreview(propertyKey, parsed);
    if (message !== undefined) {
      setError(message);
      revert();
      return;
    }
    setDraft(null);
    setError(undefined);
    finishGesture(held, propertyKey, editing);
    editing.onGestureActiveChange(false);
  }

  function cancel(): void {
    setDraft(null);
    setError(undefined);
    revert();
  }

  return (
    <div className="particles-field">
      <span className="particles-field__label">{label}</span>
      <ParticleNumberInput
        label={label}
        text={text}
        isDefault={draft === null && held.shown === undefined}
        isDisabled={editing.isDisabled}
        onTextChange={change}
        onEnter={finish}
        onBlur={finish}
        onEscape={cancel}
      />
      <FieldError message={error} />
    </div>
  );
}

type ColorSwatchProps = { propertyKey: string; fallback: string; editing: ParticleEditing };

/** Палитра браузера, как у свойства `color`: принятый цвет пишется сразу; цвета, которого у объекта нет, не видно — палитра бледная. */
export function ColorSwatch({ propertyKey, fallback, editing }: ColorSwatchProps): React.JSX.Element {
  const value = editing.properties[propertyKey];
  return (
    <span className={value === undefined ? "particles-color particles-color--default" : "particles-color"}>
      <ColorPickerInput
        key={String(value)}
        value={colorForPicker(value, fallback)}
        isDisabled={editing.isDisabled}
        onCommit={(hex) => editing.onCommit(propertyKey, hex, value)}
      />
    </span>
  );
}

type ColorFieldProps = { label: string; propertyKey: string; fallback: string; editing: ParticleEditing };

export function ColorField({ label, propertyKey, fallback, editing }: ColorFieldProps): React.JSX.Element {
  return (
    <div className="particles-field">
      <span className="particles-field__label">{label}</span>
      <ColorSwatch propertyKey={propertyKey} fallback={fallback} editing={editing} />
    </div>
  );
}
