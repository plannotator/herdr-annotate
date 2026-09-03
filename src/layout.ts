import { graphemes, stringWidth } from "./width";

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

type CommentGrapheme = {
  text: string;
  start: number;
  end: number;
  cells: number;
};

function commentGraphemes(comment: readonly string[]): CommentGrapheme[] {
  const result: CommentGrapheme[] = [];
  let start = 0;
  for (const text of graphemes(comment.join(""))) {
    let length = 0;
    for (const _character of text) length += 1;
    const end = start + length;
    result.push({ text, start, end, cells: stringWidth(text) });
    start = end;
  }
  return result;
}

export function editorViewportStart(
  currentStart: number,
  cursorRow: number,
  lineCount: number,
  visibleRows: number,
): number {
  const rows = Math.max(1, visibleRows);
  const maxStart = Math.max(0, lineCount - rows);
  const start = Math.min(currentStart, maxStart);
  if (cursorRow < start) return cursorRow;
  if (cursorRow >= start + rows) return cursorRow - rows + 1;
  return start;
}

export function cursorAtEditorCell(
  comment: readonly string[],
  editorStart: number,
  x: number,
  y: number,
  geometry: EditorGeometry,
): number | undefined {
  if (
    x < geometry.left ||
    x > geometry.left + geometry.innerWidth ||
    y < geometry.editorTop ||
    y >= geometry.editorTop + geometry.editorRows
  ) {
    return undefined;
  }

  const targetRow = editorStart + y - geometry.editorTop;
  const editing = layoutComment(comment, comment.length, geometry.innerWidth);
  if (targetRow >= editing.lines.length) return comment.length;

  let row = 0;
  let col = 0;
  const targetCol = x - geometry.left;
  for (const grapheme of commentGraphemes(comment)) {
    if (grapheme.text === "\n") {
      if (row === targetRow) return grapheme.start;
      row += 1;
      col = 0;
      continue;
    }
    if (grapheme.cells > 0 && col > 0 && col + grapheme.cells > geometry.innerWidth) {
      if (row === targetRow) return grapheme.start;
      row += 1;
      col = 0;
    }
    if (row === targetRow) {
      if (targetCol < col) return grapheme.start;
      if (grapheme.cells > 0 && targetCol < col + grapheme.cells) {
        return targetCol === col ? grapheme.start : grapheme.end;
      }
    }
    col += grapheme.cells;
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
  for (const grapheme of commentGraphemes(comment)) {
    if (col > 0 && col + grapheme.cells > safeWidth) {
      lines.push("");
      row += 1;
      col = 0;
    }
    if (cursor === grapheme.start) {
      cursorRow = row;
      cursorCol = col;
    }
    if (grapheme.text === "\n") {
      lines.push("");
      row += 1;
      col = 0;
    } else {
      lines[row] += grapheme.text;
      col += grapheme.cells;
    }
    if (cursor > grapheme.start && cursor < grapheme.end) {
      cursorRow = row;
      cursorCol = col;
    }
  }
  if (cursor >= comment.length) {
    cursorRow = row;
    cursorCol = col;
  }
  return { lines, cursorRow, cursorCol };
}
