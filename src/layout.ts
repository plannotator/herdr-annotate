import { charWidth } from "./width";

export type EditorGeometry = {
  cols: number;
  rows: number;
  left: number;
  innerWidth: number;
  selectionRows: number;
  editorRows: number;
  editorTop: number;
};

export function editorGeometry(cols: number, rows: number): EditorGeometry {
  const safeCols = Math.max(20, cols);
  const safeRows = Math.max(10, rows);
  const selectionRows = Math.max(3, Math.min(7, Math.floor((safeRows - 6) / 2)));
  return {
    cols: safeCols,
    rows: safeRows,
    left: 2,
    innerWidth: Math.max(safeCols - 4, 1),
    selectionRows,
    editorRows: Math.max(safeRows - selectionRows - 5, 1),
    editorTop: 3 + selectionRows,
  };
}

export function cursorAtEditorCell(
  comment: readonly string[],
  currentCursor: number,
  x: number,
  y: number,
  geometry: EditorGeometry,
): number | undefined {
  if (
    x < geometry.left ||
    x >= geometry.left + geometry.innerWidth ||
    y < geometry.editorTop ||
    y >= geometry.editorTop + geometry.editorRows
  ) {
    return undefined;
  }

  const editing = layoutComment(comment, currentCursor, geometry.innerWidth);
  const editorStart = Math.max(0, editing.cursorRow - geometry.editorRows + 1);
  const targetRow = editorStart + y - geometry.editorTop;
  if (targetRow >= editing.lines.length) return comment.length;

  let row = 0;
  let col = 0;
  const targetCol = x - geometry.left;
  for (let index = 0; index <= comment.length; index += 1) {
    const char = comment[index];
    const cells = char === undefined ? 0 : charWidth(char);
    if (cells > 0 && col > 0 && col + cells > geometry.innerWidth) {
      if (row === targetRow && targetCol >= col) return index;
      row += 1;
      col = 0;
    }
    if (row === targetRow) {
      if (index === comment.length || char === "\n") return index;
      if (cells > 0 && targetCol >= col && targetCol < col + cells) {
        if (cells === 2 && targetCol === col + 1) {
          let next = index + 1;
          while (next < comment.length) {
            const trailing = comment[next];
            if (trailing === undefined || trailing === "\n" || charWidth(trailing) > 0) break;
            next += 1;
          }
          return next;
        }
        return index;
      }
    }
    if (index === comment.length) break;
    if (char === "\n") {
      row += 1;
      col = 0;
    } else {
      col += cells;
    }
  }
  return comment.length;
}

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
