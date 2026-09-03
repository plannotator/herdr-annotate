import { describe, expect, test } from "bun:test";
import { isLeftMousePress, SgrMouseDecoder } from "../src/mouse";

describe("SgrMouseDecoder", () => {
  test("assembles fragmented reports without exposing fragments", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed("\x1b[<")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("0;")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("12;")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("7M")).toEqual({
      intercepted: true,
      reports: [{ action: "press", button: 0, x: 12, y: 7 }],
    });
  });

  test("parses complete press and release reports", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed("\x1b[<0;4;5M")).toEqual({
      intercepted: true,
      reports: [{ action: "press", button: 0, x: 4, y: 5 }],
    });
    expect(decoder.feed("\x1b[<0;4;5m")).toEqual({
      intercepted: true,
      reports: [{ action: "release", button: 0, x: 4, y: 5 }],
    });
  });

  test("parses fragmented legacy press and release reports", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed("\x1b[M")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed(" ")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("$%")).toEqual({
      intercepted: true,
      reports: [{ action: "press", button: 0, x: 4, y: 5 }],
    });
    expect(decoder.feed("\x1b[M#$%")).toEqual({
      intercepted: true,
      reports: [{ action: "release", button: 3, x: 4, y: 5 }],
    });
  });

  test("discards oversized fragmented SGR reports", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed(`\x1b[<${"1".repeat(65)}`)).toEqual({
      intercepted: true,
      reports: [],
    });
    expect(decoder.feed("123;")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("M")).toEqual({ intercepted: true, reports: [] });
    expect(decoder.feed("a")).toEqual({ intercepted: false, reports: [] });
  });

  test("does not intercept ordinary key input", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed("a")).toEqual({ intercepted: false, reports: [] });
  });
});

describe("isLeftMousePress", () => {
  test("accepts modified left presses and rejects side buttons", () => {
    expect(isLeftMousePress({ action: "press", button: 0, x: 1, y: 1 })).toBe(true);
    expect(isLeftMousePress({ action: "press", button: 28, x: 1, y: 1 })).toBe(true);
    expect(isLeftMousePress({ action: "press", button: 128, x: 1, y: 1 })).toBe(false);
    expect(isLeftMousePress({ action: "release", button: 0, x: 1, y: 1 })).toBe(false);
  });
});
