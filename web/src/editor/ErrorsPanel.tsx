type ErrorsPanelProps = { errorLines: string[]; warningLines: string[]; isLoading?: boolean };

/**
 * Ошибки и предупреждения проверки — «Редактор», требование 31: сначала ошибки, потом
 * предупреждения, различаются цветом; нет ни того ни другого — «Ошибок и предупреждений нет».
 * Пока идёт самая первая загрузка проекта, вместо этого — «Загрузка…» (`isLoading`).
 */
export function ErrorsPanel({ errorLines, warningLines, isLoading = false }: ErrorsPanelProps): React.JSX.Element {
  if (errorLines.length === 0 && warningLines.length === 0) {
    return <div className="errors-panel errors-panel--empty">{isLoading ? "Загрузка…" : "Ошибок и предупреждений нет"}</div>;
  }

  return (
    <div className="errors-panel">
      {errorLines.map((line, index) => (
        <div key={`error-${index}`} className="errors-panel__line errors-panel__line--error">
          {line}
        </div>
      ))}
      {warningLines.map((line, index) => (
        <div key={`warning-${index}`} className="errors-panel__line errors-panel__line--warning">
          {line}
        </div>
      ))}
    </div>
  );
}
