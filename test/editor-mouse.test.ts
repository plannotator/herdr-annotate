import { describe, expect, test } from "bun:test";
import { SgrMouseDecoder } from "../src/mouse";

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

  test("does not intercept ordinary key input", () => {
    const decoder = new SgrMouseDecoder();
    expect(decoder.feed("a")).toEqual({ intercepted: false, reports: [] });
  });
});
