import { describe, expect, it } from "vitest";
import { gameListSearch, resolveGameName } from "./gameSelection";

describe("resolveGameName", () => {
  it("берёт имя игры из query-параметра game", () => {
    expect(resolveGameName("?game=arkanoid")).toEqual({ status: "valid", name: "arkanoid" });
  });

  it("без параметра сообщает, что его нет — страница должна показать экран выбора", () => {
    expect(resolveGameName("")).toEqual({ status: "absent" });
  });

  it("отличает подозрительное имя от отсутствующего параметра, защищаясь от выхода за папку игр", () => {
    expect(resolveGameName("?game=../../etc")).toEqual({ status: "invalid", value: "../../etc" });
    expect(resolveGameName("?game=snake/../../secret")).toEqual({
      status: "invalid",
      value: "snake/../../secret",
    });
  });

  it("считает пустое значение параметра недопустимым, а не отсутствием параметра", () => {
    expect(resolveGameName("?game=")).toEqual({ status: "invalid", value: "" });
  });

  it("отбрасывает имя с пробелом, точкой или кириллицей теми же правилами, что и список игр", () => {
    expect(resolveGameName("?game=snake 2")).toEqual({ status: "invalid", value: "snake 2" });
    expect(resolveGameName("?game=my.game")).toEqual({ status: "invalid", value: "my.game" });
    expect(resolveGameName("?game=игра")).toEqual({ status: "invalid", value: "игра" });
  });
});

describe("gameListSearch", () => {
  it("убирает игру из адреса — так страница возвращается к списку", () => {
    expect(gameListSearch("?game=arkanoid")).toBe("");
  });

  it("на самом списке ничего не меняет", () => {
    expect(gameListSearch("")).toBe("");
  });

  it("убирает только игру, остальные параметры оставляет", () => {
    expect(gameListSearch("?game=snake&debug=1")).toBe("?debug=1");
  });

  it("убирает недопустимое имя игры так же, как годное — с экрана ошибки тоже нужен выход", () => {
    expect(gameListSearch("?game=../../etc")).toBe("");
  });
});
