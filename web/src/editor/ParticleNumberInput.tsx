type ParticleNumberInputProps = {
  label: string;
  text: string;
  /** Значение по умолчанию показано бледно: свойства у объекта нет. */
  isDefault: boolean;
  isDisabled: boolean;
  onTextChange: (text: string) => void;
  onEnter: () => void;
  onBlur: () => void;
  onEscape: () => void;
};

/** Числовое поле настройки частиц — «Редактор», требование 30: показывает набранный текст, а чем кончается набор, решает поле выше. */
export function ParticleNumberInput({ label, text, isDefault, isDisabled, onTextChange, onEnter, onBlur, onEscape }: ParticleNumberInputProps): React.JSX.Element {
  return (
    <input
      type="text"
      inputMode="decimal"
      aria-label={label}
      className={isDefault ? "particles-input particles-input--default" : "particles-input"}
      value={text}
      disabled={isDisabled}
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => onTextChange(event.target.value)}
      onBlur={onBlur}
      onKeyDown={(event) => {
        if (event.key === "Enter") onEnter();
        else if (event.key === "Escape") onEscape();
      }}
    />
  );
}
