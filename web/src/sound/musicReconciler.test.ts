import { describe, expect, it } from "vitest";
import { reconcileMusic, type MusicDesired, type MusicPerformed, type MusicReconcileOptions } from "./musicReconciler";

const NO_OPTIONS: MusicReconcileOptions = { brokenTrackIds: new Set(), awaitingRetry: false };

describe("reconcileMusic", () => {
  it("то же имя, уже играет — ничего не делает", () => {
    const desired: MusicDesired = { enabled: true, trackId: 2 };
    const performed: MusicPerformed = { trackId: 2, playing: true };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "none" });
  });

  it("другое имя — бросает прежний трек и заводит новый с начала", () => {
    const desired: MusicDesired = { enabled: true, trackId: 3 };
    const performed: MusicPerformed = { trackId: 2, playing: true };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "start", trackId: 3 });
  });

  it("имени нет — ставит играющий трек на паузу", () => {
    const desired: MusicDesired = { enabled: true, trackId: null };
    const performed: MusicPerformed = { trackId: 2, playing: true };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "pause" });
  });

  it("имени нет и так уже тихо — ничего не делает", () => {
    const desired: MusicDesired = { enabled: true, trackId: null };
    const performed: MusicPerformed = { trackId: 2, playing: false };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "none" });
  });

  it("звук выключен — трек ставится на паузу, даже если экран называет музыку", () => {
    const desired: MusicDesired = { enabled: false, trackId: 2 };
    const performed: MusicPerformed = { trackId: 2, playing: true };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "pause" });
  });

  it("звук снова включён — тот же трек продолжается с места, а не заводится заново", () => {
    const desired: MusicDesired = { enabled: true, trackId: 2 };
    const performed: MusicPerformed = { trackId: 2, playing: false };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "resume" });
  });

  it("сломанный трек — сверка по нему не делает ничего, даже если он должен звучать", () => {
    const desired: MusicDesired = { enabled: true, trackId: 2 };
    const performed: MusicPerformed = { trackId: 2, playing: false };
    const options: MusicReconcileOptions = { brokenTrackIds: new Set([2]), awaitingRetry: false };
    expect(reconcileMusic(desired, performed, options)).toEqual({ kind: "none" });
  });

  it("ожидание после отказа браузера — сверка пропускается целиком, пока не снята", () => {
    const desired: MusicDesired = { enabled: true, trackId: 3 };
    const performed: MusicPerformed = { trackId: 2, playing: true };
    const options: MusicReconcileOptions = { brokenTrackIds: new Set(), awaitingRetry: true };
    expect(reconcileMusic(desired, performed, options)).toEqual({ kind: "none" });
  });

  it("пауза от браузера (вкладку спрятали и вернули) — сверка продолжает тот же трек", () => {
    const desired: MusicDesired = { enabled: true, trackId: 2 };
    const performed: MusicPerformed = { trackId: 2, playing: false };
    expect(reconcileMusic(desired, performed, NO_OPTIONS)).toEqual({ kind: "resume" });
  });

  it("нужный трек сломан, а играет другой (вернулись из партии в меню со сломанной музыкой) — останавливает его", () => {
    const desired: MusicDesired = { enabled: true, trackId: 1 }; // меню, трек сломан
    const performed: MusicPerformed = { trackId: 2, playing: true }; // ещё звучит трек партии
    const options: MusicReconcileOptions = { brokenTrackIds: new Set([1]), awaitingRetry: false };
    expect(reconcileMusic(desired, performed, options)).toEqual({ kind: "pause" });
  });

  it("нужный трек сломан и ничего не играет — по-прежнему ничего не делает", () => {
    const desired: MusicDesired = { enabled: true, trackId: 1 };
    const performed: MusicPerformed = { trackId: 1, playing: false };
    const options: MusicReconcileOptions = { brokenTrackIds: new Set([1]), awaitingRetry: false };
    expect(reconcileMusic(desired, performed, options)).toEqual({ kind: "none" });
  });
});
