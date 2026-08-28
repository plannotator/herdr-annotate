import { charWidth } from "./width";

/**
 * Lay the comment out in terminal cells and report where the cursor lands.
 *
 * The cursor column is measured in cells, not characters, so that wide
 * characters do not push the reported position out of step with the screen.
 */
export function layoutComment(
  comment: readonly string[],
  cursor: number,
  width: number,
): { lines: string[]; cursorRow: number; cursorCol: number } {
  const safeWidth = Math.max(1, width);
  const lines: string[] = [""];
  let row = 0;
  let col = 0;
  let cursorRow = 0;
  let cursorCol = 0;
  for (let index = 0; index <= comment.length; index += 1) {
    const cells = index < comment.length ? charWidth(comment[index] as string) : 0;
    // A wide character must not straddle the right edge.
    if (col > 0 && col + cells > safeWidth) {
      lines.push("");
      row += 1;
      col = 0;
    }
    if (index === cursor) {
      cursorRow = row;
      cursorCol = col;
    }
    if (index === comment.length) break;
    const char = comment[index] as string;
    if (char === "\n") {
      lines.push("");
      row += 1;
      col = 0;
      continue;
    }
    lines[row] += char;
    col += cells;
  }
  return { lines, cursorRow, cursorCol };
}
