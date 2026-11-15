/** «Звук» → «Громкость»: сила звука — квадрат доли ползунка, 50 → 0,25; 0 — тишина, 100 — как есть. */
export function volumeToGain(volumePercent: number): number {
  const share = Math.min(Math.max(volumePercent, 0), 100) / 100;
  return share * share;
}
