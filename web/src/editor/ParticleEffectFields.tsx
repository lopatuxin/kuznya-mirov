import { LEAF_COLOR_START, PARTICLE_DEFAULTS, PARTICLE_LABELS, presentEffectKeys, type ParticleEffect, type ParticleEffectId } from "./particleEffects";
import { ColorField, ColorSwatch, DensityField, NumberField, type ParticleEditing } from "./ParticleFieldControls";
import { SparksDirectionDial } from "./SparksDirectionDial";

type EffectBodyProps = { editing: ParticleEditing };

function SmokeFields({ editing }: EffectBodyProps): React.JSX.Element {
  return (
    <>
      <ColorField label={PARTICLE_LABELS.smoke_color} propertyKey="smoke_color" fallback={PARTICLE_DEFAULTS.smoke_color} editing={editing} />
      <NumberField label={PARTICLE_LABELS.smoke_height} propertyKey="smoke_height" defaultValue={PARTICLE_DEFAULTS.smoke_height} editing={editing} />
    </>
  );
}

function SparksFields({ editing }: EffectBodyProps): React.JSX.Element {
  return (
    <>
      <NumberField label={PARTICLE_LABELS.sparks_reach} propertyKey="sparks_reach" defaultValue={PARTICLE_DEFAULTS.sparks_reach} editing={editing} />
      <SparksDirectionDial editing={editing} />
    </>
  );
}

function LeavesFields({ editing }: EffectBodyProps): React.JSX.Element {
  const isAutumn = editing.properties.leaf_color === undefined;
  return (
    <div className="particles-field">
      <span className="particles-field__label">{PARTICLE_LABELS.leaf_color}</span>
      <div className="particles-leaf-color">
        <ColorSwatch propertyKey="leaf_color" fallback={LEAF_COLOR_START} editing={editing} />
        <label className="particles-leaf-color__autumn">
          <input
            type="checkbox"
            checked={isAutumn}
            disabled={editing.isDisabled}
            onChange={(event) => (event.target.checked ? editing.onRemove(["leaf_color"]) : editing.onCommit("leaf_color", LEAF_COLOR_START, undefined))}
          />
          осенние вперемешку
        </label>
      </div>
    </div>
  );
}

function FireFields({ editing }: EffectBodyProps): React.JSX.Element {
  return (
    <>
      <ColorField label={PARTICLE_LABELS.fire_color} propertyKey="fire_color" fallback={PARTICLE_DEFAULTS.fire_color} editing={editing} />
      <DensityField title={PARTICLE_LABELS.fire_glow} propertyKey="fire_glow" endLabels={["без ореола", "яркий"]} defaultValue={PARTICLE_DEFAULTS.fire_glow} editing={editing} />
    </>
  );
}

const EFFECT_BODIES: Record<ParticleEffectId, (props: EffectBodyProps) => React.JSX.Element> = {
  smoke: SmokeFields,
  sparks: SparksFields,
  leaves: LeavesFields,
  fire: FireFields,
};

type ParticleEffectGroupProps = { effect: ParticleEffect; editing: ParticleEditing };

/** Группа одного эффекта выбранного объекта — «Редактор», требование 29: главный ползунок, настройки и «Убрать». */
export function ParticleEffectGroup({ effect, editing }: ParticleEffectGroupProps): React.JSX.Element {
  const Body = EFFECT_BODIES[effect.id];
  return (
    <section className="particles-group" aria-label={effect.title}>
      <div className="particles-group__head">
        <h3 className="particles-group__title">{effect.title}</h3>
        <button
          type="button"
          className="editor-button particles-group__remove"
          disabled={editing.isDisabled}
          title={`Снять с объекта ${effect.cardLabel} вместе с настройками`}
          onClick={() => editing.onRemove(presentEffectKeys(effect, editing.properties))}
        >
          Убрать
        </button>
      </div>
      <div className="particles-group__rows">
        <DensityField title={PARTICLE_LABELS[effect.mainKey]} propertyKey={effect.mainKey} endLabels={effect.densityLabels} editing={editing} />
        <Body editing={editing} />
      </div>
    </section>
  );
}
