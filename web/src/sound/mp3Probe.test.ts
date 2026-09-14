import { describe, expect, it } from "vitest";
import { id3v2HeaderSize, mp3ProbeLength } from "./mp3Probe";

function synchsafe(size: number): [number, number, number, number] {
  return [(size >> 21) & 0x7f, (size >> 14) & 0x7f, (size >> 7) & 0x7f, size & 0x7f];
}

function id3v2Header(bodySize: number, hasFooter = false): Uint8Array {
  const [b6, b7, b8, b9] = synchsafe(bodySize);
  return new Uint8Array([
    0x49,
    0x44,
    0x33, // "ID3"
    0x04,
    0x00, // версия 2.4.0
    hasFooter ? 0x10 : 0x00, // флаги: бит футера
    b6,
    b7,
    b8,
    b9,
  ]);
}

function mpegFrameBytes(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  bytes[0] = 0xff;
  bytes[1] = 0xfb; // синхрослово MPEG-фрейма, без ID3v2
  return bytes;
}

describe("id3v2HeaderSize", () => {
  it("не находит заголовок в файле без него", () => {
    expect(id3v2HeaderSize(mpegFrameBytes(20))).toBe(0);
  });

  it("не находит заголовок в файле короче десяти байт", () => {
    expect(id3v2HeaderSize(new Uint8Array([0x49, 0x44, 0x33]))).toBe(0);
  });

  it("читает synchsafe-размер заголовка и прибавляет десять байт самого заголовка", () => {
    expect(id3v2HeaderSize(id3v2Header(100))).toBe(110);
  });

  it("прибавляет десять байт футера, если флаг футера установлен", () => {
    expect(id3v2HeaderSize(id3v2Header(100, true))).toBe(120);
  });
});

describe("mp3ProbeLength", () => {
  it("без ID3v2-заголовка пробует первые ~64 КБ файла", () => {
    const bytes = mpegFrameBytes(200_000);
    expect(mp3ProbeLength(bytes)).toBe(64 * 1024);
  });

  it("с ID3v2-заголовком прибавляет его размер к ~64 КБ", () => {
    const bodySize = 1000;
    const header = id3v2Header(bodySize);
    // Тег занимает не только сам десятибайтный заголовок, но и `bodySize` байт следом за ним —
    // именно это число заголовок и объявляет своим synchsafe-размером.
    const bytes = new Uint8Array(header.length + bodySize + 200_000);
    bytes.set(header, 0);
    expect(mp3ProbeLength(bytes)).toBe(header.length + bodySize + 64 * 1024);
  });

  it("не выходит за длину самого файла, если тот короче цели пробы", () => {
    const bytes = mpegFrameBytes(1000);
    expect(mp3ProbeLength(bytes)).toBe(1000);
  });
});
