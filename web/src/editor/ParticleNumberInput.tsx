type ParticleNumberInputProps = {
  label: string;
  /** Подсказка поля: ключ файла и его смысл. */
  hint: string;
  text: string;
  /** Значение по умолчанию показано бледно: ключа в виде нет. */
  isDefault: boolean;
  isDisabled: boolean;
  onTextChange: (text: string) => void;
  onEnter: () => void;
  onBlur: () => void;
  onEscape: () => void;
};

/** Поле вида частиц — «Редактор», требование 26: показывает набранный текст, а чем кончается набор, решает строка поля вида. */
export function ParticleNumberInput({ label, hint, text, isDefault, isDisabled, onTextChange, onEnter, onBlur, onEscape }: ParticleNumberInputProps): React.JSX.Element {
  return (
    <input
      type="text"
      inputMode="decimal"
      aria-label={label}
      title={hint}
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
