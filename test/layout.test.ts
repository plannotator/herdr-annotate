import { describe, expect, test } from "bun:test";
import { layoutComment } from "../src/layout";

const chars = (text: string): string[] => Array.from(text);

describe("layoutComment", () => {
  test("reports the cursor column in cells, not characters", () => {
    const comment = chars("한글");
    expect(layoutComment(comment, comment.length, 40).cursorCol).toBe(4);
  });

  test("counts narrow characters as one cell", () => {
    const comment = chars("abc");
    expect(layoutComment(comment, comment.length, 40).cursorCol).toBe(3);
  });

  test("mixes narrow and wide characters", () => {
    const comment = chars("a한b");
    expect(layoutComment(comment, comment.length, 40).cursorCol).toBe(4);
    expect(layoutComment(comment, 2, 40).cursorCol).toBe(3);
  });

  test("never lets a wide character straddle the right edge", () => {
    const comment = chars("한글한");
    const result = layoutComment(comment, comment.length, 5);
    expect(result.lines).toEqual(["한글", "한"]);
    expect(result.cursorRow).toBe(1);
    expect(result.cursorCol).toBe(2);
  });

  test("keeps explicit newlines", () => {
    const comment = chars("한\n글");
    const result = layoutComment(comment, comment.length, 40);
    expect(result.lines).toEqual(["한", "글"]);
    expect(result.cursorRow).toBe(1);
    expect(result.cursorCol).toBe(2);
  });
});
