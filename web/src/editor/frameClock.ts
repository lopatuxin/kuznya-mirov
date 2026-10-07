/** Дольше этого кадр часы редактора не двигает: спрятанная вкладка не отдаёт растениям накопленное время рывком. */
export const MAX_FRAME_SECONDS = 0.1;

/** Секунды с прошлого кадра для `draw(dt)`: первый кадр — 0, дальше от 0 до `MAX_FRAME_SECONDS`. */
export function secondsSinceLastFrame(previousTime: number | null, time: number): number {
  if (previousTime === null) return 0;
  return Math.min(MAX_FRAME_SECONDS, Math.max(0, (time - previousTime) / 1000));
}
