#!/usr/bin/env bun
import crypto from "node:crypto";
import { copyAndArchiveAnnotations, type CopyAndArchiveOutcome } from "./archive-workflow";
import { writeClipboard } from "./clipboard";
import { notify } from "./herdr";
import { appendArchivedSet, loadAnnotations, removeAnnotationsById } from "./store";

/** The notification and exit status one copy-and-archive action reports. */
export interface CopyArchiveReport {
  readonly title: string;
  readonly body: string;
  readonly failure: boolean;
}

/**
 * Map one copy-and-archive outcome to the action's notification and exit status.
 *
 * `loadedEmpty` separates the nothing-to-do case from a real failure: both are `stay_open`,
 * but an empty store is reported like `export.ts` and exits successfully.
 */
export function copyArchiveReport(
  outcome: CopyAndArchiveOutcome,
  loadedEmpty: boolean,
): CopyArchiveReport {
  if (outcome._tag === "close") {
    return {
      title: "Annotations copied and archived",
      body: `${outcome.archivedCount} annotation${outcome.archivedCount === 1 ? "" : "s"} copied as Markdown and archived.`,
      failure: false,
    };
  }
  if (outcome._tag === "archived_active_retained") {
    return {
      title: "Copy and archive incomplete",
      body: `Copied and archived, but active annotations remain: ${outcome.message}`,
      failure: true,
    };
  }
  if (loadedEmpty) {
    return { title: "No annotations", body: "There is nothing to copy yet.", failure: false };
  }
  return { title: "Copy and archive failed", body: outcome.message, failure: true };
}

function main(): void {
  const dir = process.env.HERDR_PLUGIN_STATE_DIR;
  if (!dir) {
    const message = "HERDR_PLUGIN_STATE_DIR is not set";
    notify("Copy and archive failed", message);
    console.error(message);
    process.exit(1);
  }

  let loadedEmpty = false;
  const outcome = copyAndArchiveAnnotations({
    loadActive: () => {
      const loaded = loadAnnotations(dir);
      if (loaded.ok && loaded.value.length === 0) loadedEmpty = true;
      return loaded;
    },
    writeClipboard,
    saveArchive: (archive) => appendArchivedSet(dir, archive),
    removeActive: (annotationIds) => removeAnnotationsById(dir, annotationIds),
    createArchiveId: () => crypto.randomUUID(),
    now: () => new Date().toISOString(),
  });

  const report = copyArchiveReport(outcome, loadedEmpty);
  notify(report.title, report.body);
  if (report.failure) {
    console.error(report.body);
    process.exit(1);
  }
}

if (import.meta.main) main();
