import { useId } from "react";
import type { ParticleEffectId } from "./particleEffects";

/** Рисунок карточки — то, чем движок рисует частицы эффекта: клуб дыма, искра или лист. */
export function ParticleCardArt({ effectId }: { effectId: ParticleEffectId }): React.JSX.Element {
  const id = useId();
  if (effectId === "smoke") {
    return (
      <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
        <defs>
          <filter id={`${id}-blur`}>
            <feGaussianBlur stdDeviation="1.6" />
          </filter>
        </defs>
        <g filter={`url(#${id}-blur)`}>
          <circle cx="15" cy="23" r="8" fill="#a9a9b0" />
          <circle cx="25" cy="22" r="8.5" fill="#b4b4bb" />
          <circle cx="20" cy="15" r="8" fill="#d2d2d8" />
          <circle cx="28" cy="14" r="5.5" fill="#dcdce2" />
          <circle cx="12" cy="15" r="5" fill="#c8c8ce" />
        </g>
      </svg>
    );
  }
  if (effectId === "sparks") {
    return (
      <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
        <defs>
          <radialGradient id={`${id}-glow`}>
            <stop offset="0" stopColor="#fff6d6" />
            <stop offset="0.25" stopColor="#ffd27a" />
            <stop offset="0.55" stopColor="#ff9a3c" stopOpacity="0.6" />
            <stop offset="1" stopColor="#ff7a1a" stopOpacity="0" />
          </radialGradient>
        </defs>
        <circle cx="20" cy="20" r="15" fill={`url(#${id}-glow)`} />
        <circle cx="9" cy="11" r="2.2" fill="#ffc861" />
        <circle cx="31" cy="27" r="1.8" fill="#ffb347" />
      </svg>
    );
  }
  return (
    <svg className="particles-card__shape" viewBox="0 0 40 40" aria-hidden="true">
      <path d="M8 31 C 9 18, 19 8, 33 8 C 32 21, 22 31, 8 31 Z" fill="#7da33a" />
      <path d="M8 31 C 15 23, 22 16, 31 10" stroke="#c4d77a" strokeWidth="1.3" fill="none" />
      <path d="M24 34 C 25 28, 30 24, 36 24 C 35 30, 30 34, 24 34 Z" fill="#d9a531" />
    </svg>
  );
}
