#!/usr/bin/env bun
import crypto from "node:crypto";
import readline from "node:readline";
import { copyAndArchiveAnnotations, restoreArchivedSet } from "./archive-workflow";
import { writeClipboard } from "./clipboard";
import { sanitizeTerminalText, wrapText } from "./format";
import { copyAnnotations } from "./manager-copy";
import { stateDir } from "./paths";
import {
  appendArchivedSet,
  loadAnnotations,
  loadArchivedSets,
  mergeAnnotations,
  newestFirstAnnotations,
  newestFirstArchivedSets,
  removeAnnotationsById,
  removeArchivedSet,
} from "./store";
import type { Annotation, ArchivedAnnotationSet } from "./types";

type ManagerView = "active" | "archives";

type Confirmation =
  | { readonly _tag: "none" }
  | { readonly _tag: "clear_active" }
  | { readonly _tag: "delete_archive"; readonly archiveId: string };

function requireStateDir(): string {
  const value = stateDir();
  if (!value) {
    console.error("HERDR_PLUGIN_STATE_DIR is not set");
    process.exit(1);
  }
  return value;
}

const dir = requireStateDir();
const out = (value: string) => process.stdout.write(value);
let annotations: Annotation[] = [];
let archives: ArchivedAnnotationSet[] = [];
let activeSelected = 0;
let archiveSelected = 0;
let view: ManagerView = "active";
let status = "";
let confirmation: Confirmation = { _tag: "none" };
let finished = false;

function clampSelection(selected: number, length: number): number {
  return Math.max(0, Math.min(selected, length - 1));
}

function reloadActive(): boolean {
  const loaded = loadAnnotations(dir);
  if (!loaded.ok) {
    status = loaded.message;
    return false;
  }
  annotations = newestFirstAnnotations(loaded.value);
  activeSelected = clampSelection(activeSelected, annotations.length);
  return true;
}

function reloadArchives(): boolean {
  const loaded = loadArchivedSets(dir);
  if (!loaded.ok) {
    status = loaded.message;
    return false;
  }
  archives = newestFirstArchivedSets(loaded.value);
  archiveSelected = clampSelection(archiveSelected, archives.length);
  return true;
}

function clipped(text: string, width: number): string {
  const chars = Array.from(sanitizeTerminalText(text).replace(/\s+/g, " ").trim());
  if (chars.length <= width) return chars.join("");
  return `${chars.slice(0, Math.max(0, width - 1)).join("")}…`;
}

function writeAt(row: number, col: number, text: string): void {
  out(`\x1b[${row};${col}H${text}`);
}

function firstVisibleIndex(selected: number, length: number, visibleRows: number): number {
  return Math.max(0, Math.min(selected - Math.floor(visibleRows / 2), length - visibleRows));
}

function countLabel(count: number): string {
  return `${count} annotation${count === 1 ? "" : "s"}`;
}

function renderActive(
  rows: number,
  listWidth: number,
  detailLeft: number,
  detailWidth: number,
  listRows: number,
): void {
  const first = firstVisibleIndex(activeSelected, annotations.length, listRows);
  writeAt(
    1,
    2,
    `\x1b[1mAnnotations (${annotations.length})\x1b[0m  \x1b[2mnewest first\x1b[0m`,
  );

  if (annotations.length === 0) {
    writeAt(3, 2, "\x1b[2mNo active annotations.\x1b[0m");
    return;
  }

  annotations.slice(first, first + listRows).forEach((annotation, index) => {
    const absolute = first + index;
    const active = absolute === activeSelected;
    const label = clipped(annotation.selectedText, listWidth - 4);
    writeAt(2 + index, 2, `${active ? "\x1b[7m›" : " "} ${label}\x1b[0m`);
  });

  const current = annotations[activeSelected];
  if (!current) return;
  const source = [current.context.workspace_label, current.context.tab_label]
    .filter(Boolean)
    .join(" / ");
  writeAt(2, detailLeft, "\x1b[1mSelected text\x1b[0m");
  const selectedLines = wrapText(sanitizeTerminalText(current.selectedText), detailWidth).slice(0, 7);
  selectedLines.forEach((line, index) => writeAt(3 + index, detailLeft, `\x1b[2m${line}\x1b[0m`));
  const commentRow = 4 + Math.max(3, selectedLines.length);
  writeAt(commentRow, detailLeft, "\x1b[1mComment\x1b[0m");
  wrapText(sanitizeTerminalText(current.comment), detailWidth)
    .slice(0, Math.max(1, rows - commentRow - 4))
    .forEach((line, index) => writeAt(commentRow + 1 + index, detailLeft, line));
  const metadata = [source, new Date(current.createdAt).toLocaleString()].filter(Boolean).join("  ·  ");
  if (metadata) writeAt(rows - 2, detailLeft, `\x1b[2m${clipped(metadata, detailWidth)}\x1b[0m`);
}

function renderArchives(
  rows: number,
  listWidth: number,
  detailLeft: number,
  detailWidth: number,
  listRows: number,
): void {
  const first = firstVisibleIndex(archiveSelected, archives.length, listRows);
  writeAt(
    1,
    2,
    `\x1b[1mArchives (${archives.length})\x1b[0m  \x1b[2mnewest first\x1b[0m`,
  );

  if (archives.length === 0) {
    writeAt(3, 2, "\x1b[2mNo archived sets.\x1b[0m");
    return;
  }

  archives.slice(first, first + listRows).forEach((archive, index) => {
    const absolute = first + index;
    const active = absolute === archiveSelected;
    const label = `${new Date(archive.archivedAt).toLocaleString()} · ${countLabel(archive.annotations.length)}`;
    writeAt(
      2 + index,
      2,
      `${active ? "\x1b[7m›" : " "} ${clipped(label, listWidth - 4)}\x1b[0m`,
    );
  });

  const current = archives[archiveSelected];
  if (!current) return;
  writeAt(2, detailLeft, "\x1b[1mArchived set\x1b[0m");
  writeAt(3, detailLeft, `\x1b[2m${clipped(new Date(current.archivedAt).toLocaleString(), detailWidth)}\x1b[0m`);
  writeAt(5, detailLeft, `\x1b[1m${countLabel(current.annotations.length)}\x1b[0m`);
  const visibleAnnotations = newestFirstAnnotations(current.annotations);
  const previewRows = Math.max(1, rows - 8);
  visibleAnnotations.slice(0, previewRows).forEach((annotation, index) => {
    const label = `${index + 1}. ${annotation.selectedText}`;
    writeAt(6 + index, detailLeft, clipped(label, detailWidth));
  });
  if (visibleAnnotations.length > previewRows) {
    writeAt(rows - 2, detailLeft, `\x1b[2m… ${visibleAnnotations.length - previewRows} more\x1b[0m`);
  }
}

function footerText(): string {
  if (confirmation._tag === "clear_active") {
    return "Press Shift+D again to clear all active annotations · Esc cancel";
  }
  if (confirmation._tag === "delete_archive") {
    return "Press d again to permanently delete this archive · Esc cancel";
  }
  if (status) return status;
  if (view === "active") {
    return "j/k · y copy · c all · Shift+C copy+archive · d delete · Shift+D clear · Tab archives · q";
  }
  return "j/k · y copy · u restore · d twice delete · Tab active · q";
}

function render(): void {
  const cols = Math.max(50, process.stdout.columns || 98);
  const rows = Math.max(14, process.stdout.rows || 28);
  const listWidth = Math.max(22, Math.min(36, Math.floor(cols * 0.36)));
  const detailLeft = listWidth + 3;
  const detailWidth = Math.max(1, cols - detailLeft - 1);
  const listRows = Math.max(1, rows - 4);

  out("\x1b[2J\x1b[H\x1b[?25l");
  for (let row = 2; row < rows; row += 1) {
    writeAt(row, listWidth + 1, "\x1b[2m│\x1b[0m");
  }
  if (view === "active") {
    renderActive(rows, listWidth, detailLeft, detailWidth, listRows);
  } else {
    renderArchives(rows, listWidth, detailLeft, detailWidth, listRows);
  }
  writeAt(rows, 2, `\x1b[2m${clipped(footerText(), cols - 3)}\x1b[0m`);
}

function copy(items: readonly Annotation[]): void {
  const outcome = copyAnnotations(items, writeClipboard);
  if (outcome._tag === "stay_open") {
    status = outcome.message;
    return;
  }
  exit(0);
}

function copyAndArchive(): void {
  const outcome = copyAndArchiveAnnotations({
    loadActive: () => loadAnnotations(dir),
    writeClipboard,
    saveArchive: (archive) => appendArchivedSet(dir, archive),
    removeActive: (annotationIds) => removeAnnotationsById(dir, annotationIds),
    createArchiveId: () => crypto.randomUUID(),
    now: () => new Date().toISOString(),
  });
  if (outcome._tag === "close") exit(0);
  if (outcome._tag === "archived_active_retained") {
    reloadArchives();
    status = `Copied and archived, but active annotations remain: ${outcome.message}`;
    return;
  }
  status = outcome.message;
}

function deleteSelectedAnnotation(): void {
  const target = annotations[activeSelected];
  if (!target) return;
  const removed = removeAnnotationsById(dir, [target.id]);
  if (!removed.ok) {
    status = removed.message;
    return;
  }
  if (!reloadActive()) return;
  status = "Annotation deleted.";
}

function clearActive(): void {
  const removed = removeAnnotationsById(
    dir,
    annotations.map((annotation) => annotation.id),
  );
  if (!removed.ok) {
    status = removed.message;
    return;
  }
  confirmation = { _tag: "none" };
  if (!reloadActive()) return;
  status = "All active annotations cleared.";
}

function restoreSelectedArchive(): void {
  const target = archives[archiveSelected];
  if (!target) {
    status = "No archive selected.";
    return;
  }
  const outcome = restoreArchivedSet(target, {
    mergeActive: (items) => mergeAnnotations(dir, items),
    removeArchive: (archiveId) => removeArchivedSet(dir, archiveId),
  });
  if (outcome._tag === "stay_open") {
    status = outcome.message;
    return;
  }
  const activeReloaded = reloadActive();
  const archivesReloaded = reloadArchives();
  if (!activeReloaded || !archivesReloaded) return;
  if (outcome._tag === "restored_archive_retained") {
    status = `Annotations restored, but the archive remains: ${outcome.message}`;
    return;
  }
  status = outcome.restoredCount
    ? `${countLabel(outcome.restoredCount)} restored.`
    : "Archive removed; its annotations were already active.";
}

function deleteSelectedArchive(archiveId: string): void {
  const removed = removeArchivedSet(dir, archiveId);
  if (!removed.ok) {
    status = removed.message;
    return;
  }
  confirmation = { _tag: "none" };
  if (!reloadArchives()) return;
  status = "Archive permanently deleted.";
}

function switchView(): void {
  confirmation = { _tag: "none" };
  status = "";
  if (view === "active") {
    view = "archives";
    reloadArchives();
  } else {
    view = "active";
    reloadActive();
  }
}

function handleActiveKey(text: string, key: readline.Key): void {
  if (text === "D") {
    if (confirmation._tag === "clear_active") clearActive();
    else confirmation = { _tag: "clear_active" };
    return;
  }

  confirmation = { _tag: "none" };
  if (key.name === "up" || key.name === "k") {
    activeSelected = Math.max(0, activeSelected - 1);
  } else if (key.name === "down" || key.name === "j") {
    activeSelected = Math.min(Math.max(0, annotations.length - 1), activeSelected + 1);
  } else if (text === "C") {
    copyAndArchive();
  } else if (key.name === "y") {
    const current = annotations[activeSelected];
    copy(current ? [current] : []);
  } else if (key.name === "c") {
    copy(annotations);
  } else if (key.name === "d") {
    deleteSelectedAnnotation();
  } else if (key.name === "r") {
    if (reloadActive()) status = "Reloaded.";
  }
}

function handleArchiveKey(text: string, key: readline.Key): void {
  if (text === "d") {
    const current = archives[archiveSelected];
    if (!current) {
      confirmation = { _tag: "none" };
      status = "No archive selected.";
    } else if (
      confirmation._tag === "delete_archive" &&
      confirmation.archiveId === current.id
    ) {
      deleteSelectedArchive(current.id);
    } else {
      confirmation = { _tag: "delete_archive", archiveId: current.id };
    }
    return;
  }

  confirmation = { _tag: "none" };
  if (key.name === "up" || key.name === "k") {
    archiveSelected = Math.max(0, archiveSelected - 1);
  } else if (key.name === "down" || key.name === "j") {
    archiveSelected = Math.min(Math.max(0, archives.length - 1), archiveSelected + 1);
  } else if (key.name === "y") {
    const current = archives[archiveSelected];
    copy(current ? newestFirstAnnotations(current.annotations) : []);
  } else if (key.name === "u") {
    restoreSelectedArchive();
  } else if (key.name === "r") {
    if (reloadArchives()) status = "Reloaded.";
  }
}

function cleanup(): void {
  if (finished) return;
  finished = true;
  if (process.stdin.isTTY) process.stdin.setRawMode(false);
  out("\x1b[?25h\x1b[2J\x1b[H\x1b[?1049l");
}

function exit(code: number): never {
  cleanup();
  process.exit(code);
}

reloadActive();
reloadArchives();
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
  if (key.ctrl && key.name === "c") return exit(0);
  if (key.name === "escape") {
    if (confirmation._tag !== "none") {
      confirmation = { _tag: "none" };
      status = "";
      render();
      return;
    }
    return exit(0);
  }
  if (key.name === "q") return exit(0);
  if (key.name === "tab") {
    switchView();
    render();
    return;
  }

  status = "";
  if (view === "active") handleActiveKey(text, key);
  else handleArchiveKey(text, key);
  render();
});

out("\x1b[?1049h");
render();
