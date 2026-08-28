#!/usr/bin/env bun
import crypto from "node:crypto";
import fs from "node:fs";
import readline from "node:readline";
import { sanitizeTerminalText, wrapText } from "./format";
import { stateDir } from "./paths";
import { layoutComment } from "./layout";
import { charWidth, stringWidth, truncateToWidth } from "./width";
import type { StoreResult } from "./store";
import {
  parsePendingAnnotation,
  pendingAnnotationFromInvocation,
  type Annotation,
  type PendingAnnotation,
} from "./types";

const pendingPath = process.env.HERDR_ANNOTATE_PENDING;
let pending: PendingAnnotation;

function invocationContext(): unknown {
  try {
    return JSON.parse(process.env.HERDR_PLUGIN_CONTEXT_JSON ?? "{}");
  } catch {
    return {};
  }
}

try {
  const invocation = invocationContext();
  if (!pendingPath) {
    const fallback = pendingAnnotationFromInvocation(invocation);
    if (!fallback) throw new Error("Missing pending annotation");
    pending = fallback;
  } else {
    const decoded: unknown = JSON.parse(fs.readFileSync(pendingPath, "utf8"));
    const parsed = parsePendingAnnotation(decoded);
    if (!parsed) throw new Error("Pending annotation is invalid");
    pending = parsed;
    fs.rmSync(pendingPath, { force: true });
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

const out = (value: string) => process.stdout.write(value);
const comment: string[] = [];
let cursor = 0;
let status = "";
let finished = false;

function moveCursorVertical(delta: number): void {
  const before = comment.slice(0, cursor).join("");
  const lines = before.split("\n");
  const row = lines.length - 1;
  const col = stringWidth(lines[row] ?? "");
  const allLines = comment.join("").split("\n");
  const targetRow = Math.max(0, Math.min(allLines.length - 1, row + delta));
  let next = 0;
  for (let index = 0; index < targetRow; index += 1) next += Array.from(allLines[index] as string).length + 1;
  // Land on the character whose cell column is closest to the current one.
  let offset = 0;
  let used = 0;
  for (const char of allLines[targetRow] as string) {
    const width = charWidth(char);
    if (used + width > col) break;
    used += width;
    offset += 1;
  }
  cursor = next + offset;
}

function writeAt(row: number, col: number, text: string): void {
  out(`\x1b[${row};${col}H${text}`);
}

function render(): void {
  const cols = Math.max(20, process.stdout.columns || 86);
  const rows = Math.max(10, process.stdout.rows || 22);
  const left = 3;
  const innerWidth = Math.max(1, cols - 4);
  const selectionRows = Math.max(3, Math.min(7, Math.floor((rows - 6) / 2)));
  const editorRows = Math.max(1, rows - selectionRows - 5);
  const wrappedSelection = wrapText(sanitizeTerminalText(pending.selectedText), innerWidth);
  const selected = wrappedSelection.slice(0, selectionRows);
  const editing = layoutComment(comment, cursor, innerWidth);
  const editorStart = Math.max(0, editing.cursorRow - editorRows + 1);
  const visibleEditor = editing.lines.slice(editorStart, editorStart + editorRows);

  out("\x1b[2J\x1b[H\x1b[?25l");
  writeAt(2, left, "\x1b[1mSelected text\x1b[0m");
  selected.forEach((line, index) => writeAt(3 + index, left, `\x1b[2m${line}\x1b[0m`));
  if (wrappedSelection.length > selectionRows) {
    writeAt(2 + selectionRows, left + innerWidth - 1, "\x1b[2m…\x1b[0m");
  }

  const commentTitleRow = 3 + selectionRows;
  writeAt(commentTitleRow, left, "\x1b[1mComment\x1b[0m");
  visibleEditor.forEach((line, index) => writeAt(commentTitleRow + 1 + index, left, line));

  const footer = status || "Ctrl+S save  ·  Esc cancel  ·  Enter new line";
  writeAt(rows, left, `\x1b[2m${truncateToWidth(footer, innerWidth)}\x1b[0m`);

  const visualCursorRow = editing.cursorRow - editorStart;
  if (visualCursorRow >= 0 && visualCursorRow < editorRows) {
    writeAt(commentTitleRow + 1 + visualCursorRow, left + editing.cursorCol, "\x1b[?25h");
  }
}

function cleanup(): void {
  if (finished) return;
  finished = true;
  if (process.stdin.isTTY) process.stdin.setRawMode(false);
  out("\x1b[?25h\x1b[2J\x1b[H\x1b[?1049l");
}

function exit(code: number): void {
  cleanup();
  process.exit(code);
}

async function save(): Promise<void> {
  const value = comment.join("").trim();
  if (!value) {
    status = "Write a comment before saving.";
    render();
    return;
  }
  const dir = stateDir();
  if (!dir) {
    status = "Plugin state directory is unavailable.";
    render();
    return;
  }
  const annotation: Annotation = {
    ...pending,
    id: crypto.randomUUID(),
    comment: value,
    createdAt: new Date().toISOString(),
  };
  let saved: StoreResult<undefined>;
  try {
    const { appendAnnotation } = await import("./store");
    saved = appendAnnotation(dir, annotation);
  } catch {
    status = "Unable to save annotation.";
    render();
    return;
  }
  if (!saved.ok) {
    status = saved.message;
    render();
    return;
  }
  status = "Saved.";
  render();
  setTimeout(() => exit(0), 250);
}

process.on("exit", cleanup);
process.on("SIGTERM", () => exit(0));
try {
  process.on("SIGHUP", () => exit(0));
} catch {
  // SIGHUP is not available on every supported platform.
}
process.stdout.on("resize", render);

readline.emitKeypressEvents(process.stdin, { escapeCodeTimeout: 20 } as any);
if (process.stdin.isTTY) process.stdin.setRawMode(true);
process.stdin.resume();
process.stdin.on("keypress", (text: string, key: readline.Key) => {
  status = "";
  if (key.ctrl && key.name === "c") return exit(0);
  if (key.ctrl && key.name === "s") {
    void save();
    return;
  }
  if (key.name === "escape") return exit(0);
  if (key.name === "backspace") {
    if (cursor > 0) comment.splice(--cursor, 1);
  } else if (key.name === "delete") {
    if (cursor < comment.length) comment.splice(cursor, 1);
  } else if (key.name === "left") {
    cursor = Math.max(0, cursor - 1);
  } else if (key.name === "right") {
    cursor = Math.min(comment.length, cursor + 1);
  } else if (key.name === "up") {
    moveCursorVertical(-1);
  } else if (key.name === "down") {
    moveCursorVertical(1);
  } else if (key.name === "home") {
    while (cursor > 0 && comment[cursor - 1] !== "\n") cursor -= 1;
  } else if (key.name === "end") {
    while (cursor < comment.length && comment[cursor] !== "\n") cursor += 1;
  } else if (key.name === "return") {
    comment.splice(cursor, 0, "\n");
    cursor += 1;
  } else if (text && !key.ctrl && !key.meta) {
    const inserted = Array.from(text);
    comment.splice(cursor, 0, ...inserted);
    cursor += inserted.length;
  }
  render();
});

out("\x1b[?1049h");
render();
