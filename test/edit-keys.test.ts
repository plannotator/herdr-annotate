import { describe, expect, test } from "bun:test";
import { lineEnd, lineStart, resolveEditKey, wordEnd, wordStart } from "../src/edit-keys";

const chars = (text: string) => Array.from(text);

describe("word boundaries", () => {
  test("wordStart skips trailing spaces then the word", () => {
    const c = chars("foo bar  ");
    expect(wordStart(c, c.length)).toBe(4);
    expect(wordStart(c, 4)).toBe(0);
    expect(wordStart(c, 0)).toBe(0);
  });

  test("wordStart stops at a newline before crossing it", () => {
    const c = chars("foo\nbar");
    expect(wordStart(c, 4)).toBe(3);
    expect(wordStart(c, 3)).toBe(0);
  });

  test("wordEnd skips leading spaces then the word", () => {
    const c = chars("  foo bar");
    expect(wordEnd(c, 0)).toBe(5);
    expect(wordEnd(c, 5)).toBe(9);
    expect(wordEnd(c, 9)).toBe(9);
  });

  test("wordEnd stops after a newline", () => {
    const c = chars("foo\nbar");
    expect(wordEnd(c, 3)).toBe(4);
  });

  test("CJK runs count as one word", () => {
    const c = chars("한글 테스트");
    expect(wordStart(c, c.length)).toBe(3);
    expect(wordEnd(c, 0)).toBe(2);
  });
});

describe("line boundaries", () => {
  test("lineStart and lineEnd stay within the current line", () => {
    const c = chars("ab\ncd\nef");
    expect(lineStart(c, 4)).toBe(3);
    expect(lineEnd(c, 4)).toBe(5);
    expect(lineStart(c, 0)).toBe(0);
    expect(lineEnd(c, 8)).toBe(8);
  });
});

describe("resolveEditKey", () => {
  test("readline kill bindings (Cmd/Opt+Backspace via Ghostty)", () => {
    expect(resolveEditKey({ ctrl: true, name: "u", sequence: "\x15" })).toBe("delete-line");
    expect(resolveEditKey({ ctrl: true, name: "w", sequence: "\x17" })).toBe("delete-word");
    expect(resolveEditKey({ meta: true, name: "backspace", sequence: "\x1b\x7f" })).toBe("delete-word");
  });

  test("Option arrows move by word", () => {
    expect(resolveEditKey({ meta: true, name: "left", sequence: "\x1b[1;3D" })).toBe("word-left");
    expect(resolveEditKey({ meta: true, name: "right", sequence: "\x1b[1;3C" })).toBe("word-right");
    expect(resolveEditKey({ meta: true, name: "b", sequence: "\x1bb" })).toBe("word-left");
    expect(resolveEditKey({ meta: true, name: "f", sequence: "\x1bf" })).toBe("word-right");
  });

  test("Command arrows move to line edges", () => {
    expect(resolveEditKey({ meta: true, name: "left", sequence: "\x1b[1;9D" })).toBe("line-start");
    expect(resolveEditKey({ meta: true, name: "right", sequence: "\x1b[1;9C" })).toBe("line-end");
  });

  test("plain keys are left to the editor", () => {
    expect(resolveEditKey({ name: "left", sequence: "\x1b[D" })).toBeNull();
    expect(resolveEditKey({ name: "backspace", sequence: "\x7f" })).toBeNull();
    expect(resolveEditKey({ name: "a", sequence: "a" })).toBeNull();
  });
});
