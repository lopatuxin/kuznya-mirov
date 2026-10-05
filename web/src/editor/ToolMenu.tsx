import { useEffect, useRef, useState } from "react";
import { EditorIcon, type EditorIconName } from "./EditorIcon";

type ToolMenuProps = {
  /** Название группы — подпись на кнопке и имя окошка для чтения с экрана. */
  label: string;
  icon: EditorIconName;
  /** Подпись на кнопке; без неё кнопка — один значок. */
  caption?: string;
  /** Подсказка кнопки. */
  title: string;
  /** Инструмент группы выбран — кнопка подсвечена. */
  isActive?: boolean;
  isDisabled?: boolean;
  /**
   * Кнопка-переключатель: щелчок по ней выбирает инструмент группы, стрелка рядом открывает окошко. Без неё щелчок по
   * кнопке сам открывает окошко — так у групп без своего инструмента («Вода», меню записи).
   */
  onActivate?: () => void;
  /** Содержимое окошка; функция получает `close` — команда закрывает окошко после себя. */
  children: React.ReactNode | ((close: () => void) => React.ReactNode);
};

/** Кнопка не забирает фокус у сцены: `W`, `E`, `R` и `F` работают и после щелчка по ней. */
export function keepSceneFocus(event: React.MouseEvent): void {
  event.preventDefault();
}

/**
 * Поле окошка, в котором стоит фокус, его теряет — и принимает набранное, как при уходе из поля. Иначе окошко
 * закрылось бы раньше: React не зовёт `onBlur` у поля, которое сам убирает, и набранное без Enter пропало бы.
 */
function blurFocusInside(root: HTMLElement | null): void {
  const active = document.activeElement;
  if (active instanceof HTMLElement && root?.contains(active)) active.blur();
}

/**
 * Группа верхней полосы с выпадающим окошком: инструменты группы и их настройки не стоят в полосе постоянно, а
 * открываются стрелкой. Окошко закрывается щелчком мимо него, в том числе по сцене, и клавишей Esc.
 */
export function ToolMenu({ label, icon, caption, title, isActive = false, isDisabled = false, onActivate, children }: ToolMenuProps): React.JSX.Element {
  const [isOpen, setIsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    function closeOutside(event: PointerEvent): void {
      if (rootRef.current?.contains(event.target as Node)) return;
      blurFocusInside(rootRef.current);
      setIsOpen(false);
    }
    // Esc закрывает только окошко: до сцены и до игры он не доходит, выбранная кисть остаётся; набранное в поле не принимается.
    function closeOnEscape(event: KeyboardEvent): void {
      if (event.code !== "Escape") return;
      event.stopPropagation();
      setIsOpen(false);
    }
    document.addEventListener("pointerdown", closeOutside, true);
    document.addEventListener("keydown", closeOnEscape, true);
    return () => {
      document.removeEventListener("pointerdown", closeOutside, true);
      document.removeEventListener("keydown", closeOnEscape, true);
    };
  }, [isOpen]);

  // Группа стала недоступна (из игры пропали материалы или штампы) — её окошко закрывается.
  useEffect(() => {
    if (isDisabled) setIsOpen(false);
  }, [isDisabled]);

  const close = (): void => {
    blurFocusInside(rootRef.current);
    setIsOpen(false);
  };
  const toggle = (): void => (isOpen ? close() : setIsOpen(true));
  const content = (
    <>
      <EditorIcon name={icon} size={15} />
      {caption !== undefined && <span className="tool-menu__caption">{caption}</span>}
    </>
  );

  return (
    <div className={`tool-menu${isOpen ? " tool-menu--open" : ""}`} ref={rootRef}>
      <div className={`tool-menu__trigger${isActive ? " tool-menu__trigger--active" : ""}`}>
        {onActivate ? (
          <>
            <button type="button" className="tool-menu__main" title={title} aria-pressed={isActive} disabled={isDisabled} onMouseDown={keepSceneFocus} onClick={onActivate}>
              {content}
            </button>
            <button
              type="button"
              className="tool-menu__toggle"
              title={`${label}: настройки`}
              aria-label={`${label}: настройки`}
              aria-expanded={isOpen}
              disabled={isDisabled}
              onMouseDown={keepSceneFocus}
              onClick={toggle}
            >
              <EditorIcon name="chevron-down" size={12} />
            </button>
          </>
        ) : (
          <button type="button" className="tool-menu__main" title={title} aria-label={caption === undefined ? title : undefined} aria-expanded={isOpen} disabled={isDisabled} onMouseDown={keepSceneFocus} onClick={toggle}>
            {content}
            {caption !== undefined && <EditorIcon name="chevron-down" size={12} />}
          </button>
        )}
      </div>
      {isOpen && (
        <div className="tool-menu__popover" role="dialog" aria-label={label}>
          {typeof children === "function" ? children(close) : children}
        </div>
      )}
    </div>
  );
}

type ToolMenuItemProps = {
  /** Значок слева; без него — строка из одного названия, как материал. */
  icon?: EditorIconName;
  label: string;
  /** Пояснение справа: клавиша или что делает Shift. */
  hint?: string;
  /** Подсказка при наведении, например почему пункт неактивен. */
  title?: string;
  isActive?: boolean;
  isDisabled?: boolean;
  onSelect: () => void;
};

/** Строка окошка: инструмент группы или команда. */
export function ToolMenuItem({ icon, label, hint, title, isActive = false, isDisabled = false, onSelect }: ToolMenuItemProps): React.JSX.Element {
  return (
    <button
      type="button"
      className={`tool-menu__item${isActive ? " tool-menu__item--active" : ""}`}
      title={title}
      aria-pressed={isActive}
      disabled={isDisabled}
      onMouseDown={keepSceneFocus}
      onClick={onSelect}
    >
      {icon !== undefined && <EditorIcon name={icon} size={15} />}
      <span className="tool-menu__item-label">{label}</span>
      {hint !== undefined && <span className="tool-menu__item-hint">{hint}</span>}
    </button>
  );
}
