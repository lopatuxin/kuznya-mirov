import { useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { formatRussianCount } from "./russianPlural";

type ErrorsPanelProps = { errorLines: string[]; warningLines: string[]; isLoading?: boolean };

const LOCATION_SEPARATOR = " — ";

/** Строка `formatError` — «файл: путь — текст»; место в файле и текст показываются разным шрифтом. */
function ProblemLineText({ line }: { line: string }): React.JSX.Element {
  const separatorIndex = line.indexOf(LOCATION_SEPARATOR);
  if (separatorIndex === -1) return <span className="problem-line__message">{line}</span>;
  return (
    <span className="problem-line__text">
      <span className="problem-line__location">{line.slice(0, separatorIndex)}</span>
      <span className="problem-line__separator">{LOCATION_SEPARATOR}</span>
      <span className="problem-line__message">{line.slice(separatorIndex + LOCATION_SEPARATOR.length)}</span>
    </span>
  );
}

/**
 * Ошибки и предупреждения проверки — «Редактор», требование 31: сначала ошибки, потом
 * предупреждения, различаются цветом; нет ни того ни другого — «Ошибок и предупреждений нет».
 * Пока идёт самая первая загрузка проекта, вместо этого — «Загрузка…» (`isLoading`). Шапка со
 * счётчиками сворачивает список, чтобы он не отнимал место у сцены.
 */
export function ErrorsPanel({ errorLines, warningLines, isLoading = false }: ErrorsPanelProps): React.JSX.Element {
  const [isCollapsed, setIsCollapsed] = useState(false);

  if (errorLines.length === 0 && warningLines.length === 0) {
    return (
      <div className="problems-panel problems-panel--empty">
        {isLoading ? <span className="editor-spinner" /> : <EditorIcon name="ok" size={15} className="problems-panel__ok" />}
        {isLoading ? "Загрузка…" : "Ошибок и предупреждений нет"}
      </div>
    );
  }

  return (
    <section className="problems-panel">
      <button
        type="button"
        className="problems-panel__header"
        aria-expanded={!isCollapsed}
        onClick={() => setIsCollapsed((current) => !current)}
      >
        <span className="problems-panel__title">Проверка проекта</span>
        {errorLines.length > 0 && (
          <span className="problems-count problems-count--error">
            <EditorIcon name="error" size={13} />
            {formatRussianCount(errorLines.length, ["ошибка", "ошибки", "ошибок"])}
          </span>
        )}
        {warningLines.length > 0 && (
          <span className="problems-count problems-count--warning">
            <EditorIcon name="warning" size={13} />
            {formatRussianCount(warningLines.length, ["предупреждение", "предупреждения", "предупреждений"])}
          </span>
        )}
        <span className="problems-panel__toggle">
          {isCollapsed ? "Показать" : "Свернуть"}
          <EditorIcon name={isCollapsed ? "chevron-up" : "chevron-down"} size={14} />
        </span>
      </button>

      {!isCollapsed && (
        <ul className="problems-panel__list">
          {errorLines.map((line, index) => (
            <li key={`error-${index}`} className="problem-line problem-line--error">
              <EditorIcon name="error" size={14} />
              <ProblemLineText line={line} />
            </li>
          ))}
          {warningLines.map((line, index) => (
            <li key={`warning-${index}`} className="problem-line problem-line--warning">
              <EditorIcon name="warning" size={14} />
              <ProblemLineText line={line} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
