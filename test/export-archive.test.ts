import { describe, expect, test } from "bun:test";
import { copyAndArchiveAnnotations } from "../src/archive-workflow";
import { copyArchiveReport } from "../src/export-archive";
import type { Annotation, ArchivedAnnotationSet } from "../src/types";

function annotation(id: string): Annotation {
  return {
    id,
    selectedText: `selection ${id}`,
    comment: `comment ${id}`,
    capturedAt: "2026-08-08T00:00:00Z",
    createdAt: "2026-08-08T00:00:01Z",
    context: {},
  };
}

interface Dependencies {
  readonly active?: readonly Annotation[];
  readonly loadMessage?: string;
  readonly clipboardMessage?: string;
  readonly saveMessage?: string;
  readonly removeMessage?: string;
}

/** Drive the workflow with mocked dependencies, then report it like `src/export-archive.ts` does. */
function report(dependencies: Dependencies) {
  const active = dependencies.active ?? [];
  let loadedEmpty = false;
  const outcome = copyAndArchiveAnnotations({
    loadActive: () => {
      if (dependencies.loadMessage) return { ok: false, message: dependencies.loadMessage };
      if (active.length === 0) loadedEmpty = true;
      return { ok: true, value: [...active] };
    },
    writeClipboard: () =>
      dependencies.clipboardMessage
        ? { ok: false, message: dependencies.clipboardMessage }
        : { ok: true, value: undefined },
    saveArchive: (_archive: ArchivedAnnotationSet) =>
      dependencies.saveMessage
        ? { ok: false, message: dependencies.saveMessage }
        : { ok: true, value: undefined },
    removeActive: () =>
      dependencies.removeMessage
        ? { ok: false, message: dependencies.removeMessage }
        : { ok: true, value: undefined },
    createArchiveId: () => "archive-one",
    now: () => "2026-08-26T23:32:00.000Z",
  });
  return copyArchiveReport(outcome, loadedEmpty);
}

describe("copy-archive action reporting", () => {
  test("an empty store reports nothing to copy and succeeds", () => {
    expect(report({ active: [] })).toEqual({
      title: "No annotations",
      body: "There is nothing to copy yet.",
      failure: false,
    });
  });

  test("a complete archive reports the singular and plural counts", () => {
    expect(report({ active: [annotation("one")] })).toEqual({
      title: "Annotations copied and archived",
      body: "1 annotation copied as Markdown and archived.",
      failure: false,
    });
    expect(report({ active: [annotation("one"), annotation("two")] })).toEqual({
      title: "Annotations copied and archived",
      body: "2 annotations copied as Markdown and archived.",
      failure: false,
    });
  });

  test("a failed clipboard write fails with the clipboard message", () => {
    expect(
      report({ active: [annotation("one")], clipboardMessage: "clipboard write failed" }),
    ).toEqual({
      title: "Copy and archive failed",
      body: "clipboard write failed",
      failure: true,
    });
  });

  test("a failed load or save fails without the empty-store wording", () => {
    expect(report({ loadMessage: "annotations.jsonl is unreadable" })).toEqual({
      title: "Copy and archive failed",
      body: "annotations.jsonl is unreadable",
      failure: true,
    });
    expect(report({ active: [annotation("one")], saveMessage: "archives.jsonl is busy" })).toEqual({
      title: "Copy and archive failed",
      body: "archives.jsonl is busy",
      failure: true,
    });
  });

  test("retained active annotations report the partial archive and fail", () => {
    expect(
      report({ active: [annotation("one")], removeMessage: "store is busy" }),
    ).toEqual({
      title: "Copy and archive incomplete",
      body: "Copied and archived, but active annotations remain: store is busy",
      failure: true,
    });
  });
});
