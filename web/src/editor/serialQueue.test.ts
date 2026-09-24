import { describe, expect, it } from "vitest";
import { createSerialQueue } from "./serialQueue";

function delay<T>(value: T, ms: number): Promise<T> {
  return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}

describe("createSerialQueue", () => {
  it("задачи выполняются в порядке постановки, не наперегонки", async () => {
    const queue = createSerialQueue();
    const order: number[] = [];

    const first = queue.run(async () => {
      order.push(1);
      await delay(undefined, 20);
      order.push(2);
    });
    const second = queue.run(async () => {
      order.push(3);
    });

    await Promise.all([first, second]);
    expect(order).toEqual([1, 2, 3]);
  });

  it("отказ одной задачи не блокирует следующую", async () => {
    const queue = createSerialQueue();
    const failing = queue.run(async () => {
      throw new Error("плохо");
    });
    const next = queue.run(async () => "ок");

    await expect(failing).rejects.toThrow("плохо");
    await expect(next).resolves.toBe("ок");
  });
});
