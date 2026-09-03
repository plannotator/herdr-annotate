import { describe, expect, test } from "bun:test";
import { charWidth, stringWidth, truncateToWidth } from "../src/width";

describe("charWidth", () => {
  test("gives Hangul syllables two cells", () => {
    expect(charWidth("한")).toBe(2);
    expect(charWidth("ㄱ")).toBe(2);
  });

  test("gives CJK and fullwidth forms two cells", () => {
    expect(charWidth("漢")).toBe(2);
    expect(charWidth("あ")).toBe(2);
    expect(charWidth("Ａ")).toBe(2);
  });

  test("gives Latin and punctuation one cell", () => {
    expect(charWidth("a")).toBe(1);
    expect(charWidth("·")).toBe(1);
  });

  test("gives control characters no cells", () => {
    expect(charWidth("\u0007")).toBe(0);
  });
});

describe("stringWidth", () => {
  test("adds the cells of each character", () => {
    expect(stringWidth("한글abc")).toBe(7);
    expect(stringWidth("")).toBe(0);
  });

  test("measures emoji grapheme clusters", () => {
    for (const text of ["🇺🇸", "👨‍👩‍👧‍👦", "1️⃣"]) expect(stringWidth(text)).toBe(2);
  });
});

describe("truncateToWidth", () => {
  test("never splits a wide character", () => {
    expect(truncateToWidth("한글", 3)).toBe("한");
    expect(truncateToWidth("한글", 4)).toBe("한글");
  });

  test("never splits an emoji grapheme cluster", () => {
    expect(truncateToWidth("🇺🇸x", 2)).toBe("🇺🇸");
  });

  test("returns nothing when no cells are available", () => {
    expect(truncateToWidth("한", 0)).toBe("");
  });
});
