import { describe, expect, it } from "vitest";
import { parseTickResult, type RawTickResult } from "./tickResult";

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

  it("running:false с exit — игра просит уйти к списку игр", () => {
    expect(parseTickResult({ running: false, exit: true })).toEqual({ status: "exit" });
  });

  it("код ошибки приоритетнее exit", () => {
    const error = { file: "code.lua", path: "", message: "ошибка", line: null, column: null };
    expect(parseTickResult({ running: false, error, exit: true } as unknown as RawTickResult)).toEqual({ status: "error", error });
  });
});
