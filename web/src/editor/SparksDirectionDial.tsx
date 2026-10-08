import { useRef, useState } from "react";
import { describeSparksFan, MAX_SPARKS_SPREAD, PARTICLE_DEFAULTS, sparksDirectionAtPoint, sparksSpreadAtPoint } from "./particleEffects";
import { FieldError, finishGesture, useHeldValue, type ParticleEditing } from "./ParticleFieldControls";

type DialPart = "direction" | "spread";

const DIAL_SIZE = 100;
const CENTER = DIAL_SIZE / 2;
const FAN_RADIUS = 44;
const ARROW_RADIUS = 30;

const KEY_OF_PART = { direction: "sparks_direction", spread: "sparks_spread" } as const;

function pointAt(degrees: number, radius: number): [number, number] {
  const radians = (degrees * Math.PI) / 180;
  return [CENTER + radius * Math.sin(radians), CENTER - radius * Math.cos(radians)];
}

function fanPath(direction: number, spread: number): string {
  const [fromX, fromY] = pointAt(direction - spread, FAN_RADIUS);
  const [toX, toY] = pointAt(direction + spread, FAN_RADIUS);
  return `M${CENTER} ${CENTER} L${fromX} ${fromY} A${FAN_RADIUS} ${FAN_RADIUS} 0 ${spread > 90 ? 1 : 0} 1 ${toX} ${toY} Z`;
}

function numberOf(value: unknown, fallback: number): number {
  return typeof value === "number" ? value : fallback;
}

type SparksDirectionDialProps = { editing: ParticleEditing };

/**
 * Круг направления и разброса искр — «Редактор», требование 29: конец стрелки тянут — меняется `sparks_direction`, край
 * веера — `sparks_spread`; с Ctrl — шагом 15°. Под кругом то же словами. Круг работает, как поля: на каждое движение
 * сцена меняется сразу, запись — при отпускании.
 */
export function SparksDirectionDial({ editing }: SparksDirectionDialProps): React.JSX.Element {
  const directionValue = editing.properties.sparks_direction;
  const spreadValue = editing.properties.sparks_spread;
  const directionHold = useHeldValue<number>(typeof directionValue === "number" ? directionValue : undefined);
  const spreadHold = useHeldValue<number>(typeof spreadValue === "number" ? spreadValue : undefined);
  const [error, setError] = useState<string | undefined>(undefined);
  const dragRef = useRef<DialPart | null>(null);
  const direction = numberOf(directionHold.shown, PARTICLE_DEFAULTS.sparks_direction);
  const spread = numberOf(spreadHold.shown, PARTICLE_DEFAULTS.sparks_spread);
  const [arrowX, arrowY] = pointAt(direction, ARROW_RADIUS);
  // Бледность по показанному, а не по свойствам объекта: с первого движения круг уже показывает своё значение.
  const isPale = directionHold.shown === undefined && spreadHold.shown === undefined;

  function handlePointerDown(event: React.PointerEvent<SVGSVGElement>): void {
    if (editing.isDisabled || !(event.target instanceof Element)) return;
    const part = event.target.closest("[data-part]")?.getAttribute("data-part");
    if (part !== "direction" && part !== "spread") return;
    dragRef.current = part;
    editing.onGestureActiveChange(true);
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // указателя, который можно захватить, уже нет
    }
    event.preventDefault();
  }

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>): void {
    const part = dragRef.current;
    if (part === null) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const dx = event.clientX - (bounds.left + bounds.width / 2);
    const dy = event.clientY - (bounds.top + bounds.height / 2);
    const next = part === "direction" ? sparksDirectionAtPoint(dx, dy, event.ctrlKey) : sparksSpreadAtPoint(dx, dy, direction, event.ctrlKey);
    const held = part === "direction" ? directionHold : spreadHold;
    if (held.move(next)) setError(editing.onPreview(KEY_OF_PART[part], next));
  }

  function handlePointerUp(): void {
    const part = dragRef.current;
    dragRef.current = null;
    if (part === null) return;
    const held = part === "direction" ? directionHold : spreadHold;
    if (error === undefined) {
      finishGesture(held, KEY_OF_PART[part], editing);
    } else {
      const original = held.original;
      held.drop();
      editing.onPreview(KEY_OF_PART[part], original);
    }
    editing.onGestureActiveChange(false);
  }

  return (
    <div className="particles-field">
      <span className="particles-field__label">направление и разброс</span>
      <div className="particles-dial-box">
        <svg
          className={isPale ? "particles-dial particles-dial--default" : "particles-dial"}
          viewBox={`0 0 ${DIAL_SIZE} ${DIAL_SIZE}`}
          role="group"
          aria-label="направление и разброс искр"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerUp}
        >
          <circle className="particles-dial__ring" cx={CENTER} cy={CENTER} r={FAN_RADIUS} />
          {spread >= MAX_SPARKS_SPREAD ? (
            <circle className="particles-dial__fan" cx={CENTER} cy={CENTER} r={FAN_RADIUS} />
          ) : (
            <path className="particles-dial__fan" d={fanPath(direction, spread)} />
          )}
          <line className="particles-dial__arrow" x1={CENTER} y1={CENTER} x2={arrowX} y2={arrowY} />
          {(["from", "to"] as const).map((edge) => {
            const [x, y] = pointAt(direction + (edge === "from" ? -spread : spread), FAN_RADIUS);
            return <circle key={edge} className="particles-dial__handle particles-dial__handle--spread" data-part="spread" cx={x} cy={y} r={4.5} />;
          })}
          <circle className="particles-dial__handle particles-dial__handle--direction" data-part="direction" cx={arrowX} cy={arrowY} r={5.5} />
        </svg>
        <span className="particles-dial__text">{describeSparksFan(direction, spread)}</span>
      </div>
      <FieldError message={error} />
    </div>
  );
}
