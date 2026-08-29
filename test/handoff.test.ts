import { describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { HANDOFF_MAX_AGE_MS, handoffPath, takeHandoff } from "../src/handoff";

function tempFile(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "herdr-annotate-handoff-"));
  return path.join(dir, "selection");
}

describe("handoffPath", () => {
  test("prefers XDG_RUNTIME_DIR and is per user", () => {
    const p = handoffPath({ XDG_RUNTIME_DIR: "/run/user/1000" });
    expect(p.startsWith(path.join("/run/user/1000", "herdr-annotate-"))).toBe(true);
    expect(path.basename(p)).toBe("selection");
  });
  test("falls back to the temp dir", () => {
    expect(handoffPath({}).startsWith(os.tmpdir())).toBe(true);
  });
});

describe("takeHandoff", () => {
  test("returns a fresh file's text and removes it", () => {
    const file = tempFile();
    fs.writeFileSync(file, "hello\nworld\n");
    expect(takeHandoff(file)).toBe("hello\nworld\n");
    expect(fs.existsSync(file)).toBe(false);
  });
  test("ignores and removes a stale file", () => {
    const file = tempFile();
    fs.writeFileSync(file, "old");
    const later = Date.now() + HANDOFF_MAX_AGE_MS + 1000;
    expect(takeHandoff(file, later)).toBeUndefined();
    expect(fs.existsSync(file)).toBe(false);
  });
  test("ignores blank text and a missing file", () => {
    const file = tempFile();
    fs.writeFileSync(file, "  \n");
    expect(takeHandoff(file)).toBeUndefined();
    expect(takeHandoff(file)).toBeUndefined();
  });
});
