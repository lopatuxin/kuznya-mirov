import { describeDeleteCause, describeRuleKind, type StepReport, type StepReportRuleEntry } from "./battleTypes";

type StepReportPanelProps = {
  /** Последний сделанный шаг — в повторе шаг, на котором стоит шкала; `undefined` до первого шага. */
  report: StepReport | undefined;
  onSelectObject: (id: number) => void;
};

function ObjectRef({ id, name, onSelect }: { id: number; name: string | null; onSelect: (id: number) => void }): React.JSX.Element {
  return (
    <button type="button" className="step-report__object-ref" onClick={() => onSelect(id)}>
      № {id}
      {name !== null && ` (${name})`}
    </button>
  );
}

function RuleObjects({ entry, onSelectObject }: { entry: StepReportRuleEntry; onSelectObject: (id: number) => void }): React.JSX.Element {
  if (entry.kind === "collide") {
    return (
      <>
        {entry.pairs.map(([first, second]) => (
          <span key={`${first}-${second}`} className="step-report__pair">
            <ObjectRef id={first} name={null} onSelect={onSelectObject} /> × <ObjectRef id={second} name={null} onSelect={onSelectObject} />
          </span>
        ))}
      </>
    );
  }
  return (
    <>
      {entry.objects.map((id) => (
        <ObjectRef key={id} id={id} name={null} onSelect={onSelectObject} />
      ))}
    </>
  );
}

/**
 * Вкладка «Шаг» — «Редактор», требования 23–25: правила, сработавшие на шаге, созданные и удалённые
 * объекты с причиной, смена экрана и исход. Щелчок по номеру объекта выбирает его (требование 24) —
 * вызывающая сторона сама решает, жив ли он ещё.
 */
export function StepReportPanel({ report, onSelectObject }: StepReportPanelProps): React.JSX.Element {
  if (report === undefined) {
    return <div className="problems-panel problems-panel--empty">Шагов ещё не было</div>;
  }

  return (
    <div className="step-report">
      <div className="step-report__heading">Шаг {report.step}</div>

      {report.rules.length === 0 && report.created.length === 0 && report.deleted.length === 0 && report.screenChange === null && report.outcome === null ? (
        <div className="step-report__empty">На этом шаге ничего не сработало</div>
      ) : (
        <ul className="step-report__list">
          {report.rules.map((entry, index) => (
            <li key={`${entry.rule}-${index}`}>
              <span className="step-report__rule">{entry.rule}</span> {describeRuleKind(entry.kind)}: <RuleObjects entry={entry} onSelectObject={onSelectObject} />
            </li>
          ))}
          {report.created.map((created) => (
            <li key={`created-${created.id}`}>
              Создан <ObjectRef id={created.id} name={created.name} onSelect={onSelectObject} /> — {created.rule}
            </li>
          ))}
          {report.deleted.map((deleted) => (
            <li key={`deleted-${deleted.id}`}>
              Удалён № {deleted.id}
              {deleted.name !== null && ` (${deleted.name})`} — {describeDeleteCause(deleted.cause)}
            </li>
          ))}
          {report.screenChange !== null && (
            <li>
              Экран «{report.screenChange.from}» → «{report.screenChange.to}»
            </li>
          )}
          {report.outcome !== null && <li>Партия окончена: {report.outcome === "win" ? "победа" : "поражение"}</li>}
        </ul>
      )}
    </div>
  );
}
