import { useState } from "react";
import { PARTICLE_FIELD_GROUPS, type ParticleFieldSpec } from "./particleFieldSpecs";
import { particleFieldText, parseParticleFieldText, type ParticleFieldValue } from "./particleFieldValues";
import { ParticleNumberInput } from "./ParticleNumberInput";
import type { ParticleKind } from "./particlesFile";

/** Ключи, у которых во вкладке есть своё поле. */
const FIELD_KEYS = new Set(PARTICLE_FIELD_GROUPS.flatMap((group) => group.fields.map((field) => field.key)));

type Rejection = { message: string | null; set: (message: string) => void; clear: () => void };

/** Ошибка не принятого значения поля: живёт, пока действующее значение поля то же, каким оно было при отказе. */
function useRejection(valueKey: string): Rejection {
  const [rejection, setRejection] = useState<{ message: string; valueWhenRejected: string } | null>(null);
  if (rejection !== null && rejection.valueWhenRejected !== valueKey) setRejection(null);
  return {
    message: rejection?.message ?? null,
    set: (message) => setRejection({ message, valueWhenRejected: valueKey }),
    clear: () => setRejection(null),
  };
}

type ParticleFieldRowProps = {
  spec: ParticleFieldSpec;
  value: unknown;
  isDisabled: boolean;
  error: string | null;
  onPreview: (key: string, value: ParticleFieldValue) => void;
  /** Текст ошибки, если значение не принято; `undefined` — принято. */
  onCommit: (key: string, value: ParticleFieldValue) => string | undefined;
  onRevert: (key: string) => void;
};

/** Строка поля вида — подпись и одно поле ввода; ошибка — под ними во всю ширину колонки. */
function ParticleFieldRow({ spec, value, isDisabled, error, onPreview, onCommit, onRevert }: ParticleFieldRowProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null);
  // Принятое, но ещё не показанное значением: правки идут в очередь, и пока она занята, `value` старое — без этого текста
  // поле на миг показало бы прежнее число.
  const [accepted, setAccepted] = useState<{ text: string; valueWhenAccepted: string } | null>(null);
  const valueKey = String(JSON.stringify(value));
  const rejection = useRejection(valueKey);
  if (accepted !== null && accepted.valueWhenAccepted !== valueKey) setAccepted(null);
  const { text, isDefault } = particleFieldText(spec, value);
  const pendingText = accepted?.text ?? null;
  const shownText = draft ?? pendingText ?? text;
  const shownError = rejection.message ?? error;

  // Не принятое число стоит в поле как принятое, пока его не сменили набором или Esc: от принятого его отличает только текст ошибки.
  function dropRejection(): void {
    rejection.clear();
    setAccepted(null);
  }

  function change(next: string): void {
    if (rejection.message !== null) dropRejection();
    setDraft(next);
    const parsed = parseParticleFieldText(spec, next);
    if (parsed.isValid) onPreview(spec.key, parsed.value);
  }

  function finish(): void {
    if (draft === null) return;
    setDraft(null);
    const parsed = parseParticleFieldText(spec, draft);
    if (!parsed.isValid) {
      onRevert(spec.key);
      return;
    }
    const error = onCommit(spec.key, parsed.value);
    if (String(JSON.stringify(parsed.value)) === valueKey) return;
    setAccepted({ text: draft, valueWhenAccepted: valueKey });
    if (error !== undefined) rejection.set(error);
  }

  function cancel(): void {
    if (draft === null && rejection.message === null) return;
    setDraft(null);
    if (rejection.message !== null) dropRejection();
    onRevert(spec.key);
  }

  // Обёртка не занимает места (`display: contents`): подпись, поле и ошибка стоят прямо в сетке колонки, а подсказка и
  // ошибка остаются при своей строке.
  return (
    <div className="particles-field" title={spec.hint}>
      <span className="particles-field__label">{spec.label}</span>
      <ParticleNumberInput
        label={spec.label}
        hint={spec.hint}
        text={shownText}
        isDefault={isDefault && draft === null && pendingText === null}
        isDisabled={isDisabled}
        onTextChange={change}
        onEnter={finish}
        onBlur={finish}
        onEscape={cancel}
      />
      {shownError !== null && (
        <div className="particles-field__error" role="alert">
          {shownError}
        </div>
      )}
    </div>
  );
}

type ParticleKindFieldsProps = {
  kind: ParticleKind;
  /** Имя вида — над колонками, как в наброске вкладки; у своего вида — с правкой. */
  title: React.ReactNode;
  isDisabled: boolean;
  /** Ошибки проверки загрузкой по ключам полей этого вида: стоят у своего поля, пока файл их даёт. */
  loadErrors: Readonly<Record<string, string>>;
  /** Ставит виду набранное значение на сцене; текст ошибки движка, если он значение не принял. */
  onPreview: (key: string, value: unknown) => string | undefined;
  /** Принимает значение действием; текст ошибки, если оно не принято сразу. */
  onCommit: (key: string, value: unknown) => string | undefined;
  /** Возвращает на сцене виды, как они показаны во вкладке. */
  onRevert: () => void;
};

/**
 * Поля выбранного вида — «Редактор», требования 25–27: сверху имя, ниже три колонки «Вылет», «Вид»,
 * «Полёт» — подпись и поле рядом, без рамок, по ширине содержимого. Что ввод принят движком, видно сразу на сцене; ошибка
 * стоит у поля, которое её дало: отказ принять значение держит строка поля, пока оно показано, ошибку предпросмотра — этот
 * список по ключам, ошибку проверки загрузкой дают `loadErrors`.
 */
export function ParticleKindFields({ kind, title, isDisabled, loadErrors, onPreview, onCommit, onRevert }: ParticleKindFieldsProps): React.JSX.Element {
  const [previewErrors, setPreviewErrors] = useState<Readonly<Record<string, string | undefined>>>({});

  const reportPreview = (key: string, message: string | undefined): void => setPreviewErrors((current) => ({ ...current, [key]: message }));
  const errorOf = (key: string): string | null => previewErrors[key] ?? loadErrors[key] ?? null;
  // Ошибки без своего поля — на весь вид («и image, и shape»), картинки, рисунка или неизвестного ключа, записанных в файл
  // руками, — стоят под именем вида («Редактор», требование 26).
  const kindError = Object.entries(loadErrors).find(([key]) => !FIELD_KEYS.has(key))?.[1] ?? null;

  function preview(key: string, value: unknown): void {
    reportPreview(key, onPreview(key, value));
  }

  function commit(key: string, value: unknown): string | undefined {
    if (JSON.stringify(kind.fields[key]) === JSON.stringify(value)) {
      revert(key);
      return undefined;
    }
    reportPreview(key, undefined);
    return onCommit(key, value);
  }

  function revert(key: string): void {
    reportPreview(key, undefined);
    onRevert();
  }

  return (
    <div className="particles-fields">
      <div className="particles-head">{title}</div>
      {kindError !== null && (
        <div className="particles-field__error" role="alert">
          {kindError}
        </div>
      )}
      <div className="particles-groups">
        {PARTICLE_FIELD_GROUPS.map((group) => (
          <section key={group.title} className="particles-group" aria-label={group.title}>
            <h3 className="particles-group__title">{group.title}</h3>
            <div className="particles-group__rows">
              {group.fields.map((spec) => (
                <ParticleFieldRow
                  key={spec.key}
                  spec={spec}
                  value={kind.fields[spec.key]}
                  isDisabled={isDisabled}
                  error={errorOf(spec.key)}
                  onPreview={preview}
                  onCommit={commit}
                  onRevert={revert}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
