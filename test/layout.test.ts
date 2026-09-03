import { describe, expect, test } from "bun:test";
import { cursorAtEditorCell, editorGeometry, layoutComment } from "../src/layout";

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

describe("cursorAtEditorCell", () => {
  const geometry = editorGeometry(20, 12);
  const chars = (text: string): string[] => Array.from(text);

  test("maps narrow glyph cells and the line end", () => {
    const comment = chars("abc");
    expect(cursorAtEditorCell(comment, 0, geometry.left, geometry.editorTop, geometry)).toBe(0);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 1, geometry.editorTop, geometry)).toBe(1);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 5, geometry.editorTop, geometry)).toBe(3);
  });

  test("maps wide glyph cells to before and after positions", () => {
    const comment = chars("a한b");
    expect(cursorAtEditorCell(comment, 0, geometry.left + 1, geometry.editorTop, geometry)).toBe(1);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 2, geometry.editorTop, geometry)).toBe(2);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 3, geometry.editorTop, geometry)).toBe(2);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 4, geometry.editorTop, geometry)).toBe(3);
  });

  test("keeps combining marks attached to their base glyph", () => {
    const narrow = chars("a\u0301b");
    expect(cursorAtEditorCell(narrow, 0, geometry.left + 1, geometry.editorTop, geometry)).toBe(2);
    const wide = chars("한\u0301b");
    expect(cursorAtEditorCell(wide, 0, geometry.left + 1, geometry.editorTop, geometry)).toBe(2);
  });

  test("maps wrapped rows", () => {
    const narrow = editorGeometry(20, 12);
    const comment = chars("abcdefghijklmnopq");
    expect(cursorAtEditorCell(comment, 0, narrow.left, narrow.editorTop + 1, narrow)).toBe(16);
    expect(cursorAtEditorCell(comment, 0, narrow.left + 1, narrow.editorTop + 1, narrow)).toBe(17);
  });

  test("maps explicit newline rows", () => {
    const comment = chars("ab\ncd");
    expect(cursorAtEditorCell(comment, 0, geometry.left + 4, geometry.editorTop, geometry)).toBe(2);
    expect(cursorAtEditorCell(comment, 0, geometry.left, geometry.editorTop + 1, geometry)).toBe(3);
    expect(cursorAtEditorCell(comment, 0, geometry.left + 4, geometry.editorTop + 1, geometry)).toBe(5);
  });

  test("uses the pre-click cursor to preserve scroll offset", () => {
    const comment = chars("a\nb\nc\nd");
    const scrolled = editorGeometry(20, 10);
    expect(cursorAtEditorCell(comment, comment.length, scrolled.left, scrolled.editorTop, scrolled)).toBe(4);
    expect(cursorAtEditorCell(comment, comment.length, scrolled.left, scrolled.editorTop + 1, scrolled)).toBe(6);
  });

  test("returns the buffer end for blank rows and rejects outside clicks", () => {
    const comment = chars("a");
    expect(cursorAtEditorCell(comment, 0, geometry.left + 5, geometry.editorTop + 1, geometry)).toBe(1);
    expect(cursorAtEditorCell(comment, 0, geometry.left - 1, geometry.editorTop, geometry)).toBeUndefined();
    expect(cursorAtEditorCell(comment, 0, geometry.left, geometry.editorTop - 1, geometry)).toBeUndefined();
    expect(cursorAtEditorCell(comment, 0, geometry.left + geometry.innerWidth, geometry.editorTop, geometry)).toBeUndefined();
  });
});
