/**
 * Признак адреса `webgl2` («Фаза 14», требование 18): `index.html?game=rpg&webgl2`,
 * `editor.html?project=rpg&webgl2` — без значения, рядом с прочими параметрами.
 */
export function hasWebgl2Flag(search: string): boolean {
  return new URLSearchParams(search).has("webgl2");
}

/**
 * С признаком `webgl2` страница прячет `navigator.gpu` от движка до его запуска —
 * `wgpu::util::new_instance_with_webgpu_detection` не находит WebGPU и берёт WebGL2. Должно
 * выполниться до того, как движок начнёт заводиться, иначе способ рисования уже выбран.
 */
export function hideWebGpuIfRequested(search: string): void {
  if (!hasWebgl2Flag(search)) return;
  Object.defineProperty(navigator, "gpu", { value: undefined, configurable: true });
}

/** Строка в консоль после запуска движка — требование 19: каким способом он рисует. */
export function logEngineBackend(engine: { backend(): string }): void {
  console.log(`движок рисует через ${engine.backend()}`);
}
