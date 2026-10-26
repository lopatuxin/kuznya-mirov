import { BUTTERFLY_COLOR_START, LEAF_COLOR_START, PARTICLE_DEFAULTS, PARTICLE_LABELS, presentEffectKeys, type ParticleEffect, type ParticleEffectId } from "./particleEffects";
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

type VariedColorFieldProps = { label: string; propertyKey: string; startColor: string; variedLabel: string; editing: ParticleEditing };

/** Цвет эффекта, у которого без свойства цвета разные: галочка «вперемешку» стоит, пока свойства нет; снять её — записать начальный цвет. */
function VariedColorField({ label, propertyKey, startColor, variedLabel, editing }: VariedColorFieldProps): React.JSX.Element {
  const isVaried = editing.properties[propertyKey] === undefined;
  return (
    <div className="particles-field">
      <span className="particles-field__label">{label}</span>
      <div className="particles-leaf-color">
        <ColorSwatch propertyKey={propertyKey} fallback={startColor} editing={editing} />
        <label className="particles-leaf-color__autumn">
          <input
            type="checkbox"
            checked={isVaried}
            disabled={editing.isDisabled}
            onChange={(event) => (event.target.checked ? editing.onRemove([propertyKey]) : editing.onCommit(propertyKey, startColor, undefined))}
          />
          {variedLabel}
        </label>
      </div>
    </div>
  );
}

function LeavesFields({ editing }: EffectBodyProps): React.JSX.Element {
  return <VariedColorField label={PARTICLE_LABELS.leaf_color} propertyKey="leaf_color" startColor={LEAF_COLOR_START} variedLabel="осенние вперемешку" editing={editing} />;
}

function FluffFields({ editing }: EffectBodyProps): React.JSX.Element {
  return <ColorField label={PARTICLE_LABELS.fluff_color} propertyKey="fluff_color" fallback={PARTICLE_DEFAULTS.fluff_color} editing={editing} />;
}

function ButterfliesFields({ editing }: EffectBodyProps): React.JSX.Element {
  return <VariedColorField label={PARTICLE_LABELS.butterfly_color} propertyKey="butterfly_color" startColor={BUTTERFLY_COLOR_START} variedLabel="разные" editing={editing} />;
}

function FireFields({ editing }: EffectBodyProps): React.JSX.Element {
  return (
    <>
      <ColorField label={PARTICLE_LABELS.fire_color} propertyKey="fire_color" fallback={PARTICLE_DEFAULTS.fire_color} editing={editing} />
      <DensityField title={PARTICLE_LABELS.fire_glow} propertyKey="fire_glow" endLabels={["без ореола", "яркий"]} defaultValue={PARTICLE_DEFAULTS.fire_glow} editing={editing} />
      <DensityField title={PARTICLE_LABELS.fire_coals} propertyKey="fire_coals" endLabels={["нет", "высокая куча"]} defaultValue={PARTICLE_DEFAULTS.fire_coals} editing={editing} />
      <DensityField title={PARTICLE_LABELS.fire_smolder} propertyKey="fire_smolder" endLabels={["нет", "сильное"]} defaultValue={PARTICLE_DEFAULTS.fire_smolder} editing={editing} />
    </>
  );
}

const EFFECT_BODIES: Record<ParticleEffectId, (props: EffectBodyProps) => React.JSX.Element> = {
  smoke: SmokeFields,
  sparks: SparksFields,
  leaves: LeavesFields,
  fluff: FluffFields,
  butterflies: ButterfliesFields,
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
