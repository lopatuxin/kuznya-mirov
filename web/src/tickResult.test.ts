import { describe, expect, it } from "vitest";
import { parseTickResult } from "./tickResult";

describe("parseTickResult", () => {
  it("running:true — игра идёт дальше", () => {
    expect(parseTickResult({ running: true })).toEqual({ status: "running" });
  });

  it("running:false с outcome и step — партия закончилась", () => {
    expect(parseTickResult({ running: false, outcome: "win", step: 42 })).toEqual({
      status: "ended",
      outcome: "win",
      step: 42,
    });
  });

  it("running:false с error — ошибка кода останавливает партию", () => {
    const error = {
      file: "code.lua",
      path: "",
      message: 'функция "paddle_bounce", правило rules.json → rules[3], шаг 16: неизвестное свойство "sise"',
      line: 9,
      column: null,
    };
    expect(parseTickResult({ running: false, error })).toEqual({ status: "error", error });
  });
});
