#!/usr/bin/env bun
import fs from "node:fs";
import path from "node:path";
import { notify, runHerdr } from "./herdr";
import { normalizeWindowsPath, pluginRoot, stateDir } from "./paths";
import {
  parseInvocationContext,
  selectedTextFromInvocation,
  type PendingAnnotation,
} from "./types";

try {
  let context = parseInvocationContext(undefined);
  let selectedText: string | undefined;
  try {
    const decoded: unknown = JSON.parse(process.env.HERDR_PLUGIN_CONTEXT_JSON ?? "{}");
    context = parseInvocationContext(decoded);
    selectedText = selectedTextFromInvocation(decoded);
  } catch {}

  const dir = stateDir();
  if (!dir) throw new Error("HERDR_PLUGIN_STATE_DIR is not set");
  const root = pluginRoot();
  if (!root) throw new Error("HERDR_PLUGIN_ROOT is not set");

  if (!selectedText) {
    // A program on the server (Neovim's mapping) may have handed the selection over in a
    // file; that beats the clipboard, which a headless server does not have.
    const { takeHandoff } = await import("./handoff");
    selectedText = takeHandoff();
  }
  if (!selectedText) {
    const { readClipboard } = await import("./clipboard");
    const clipboard = readClipboard();
    if (!clipboard.ok) throw new Error(clipboard.message);
    selectedText = clipboard.value;
  }
  if (!selectedText.trim()) {
    notify("Nothing to annotate", "Select text in Herdr or copy text to the clipboard.");
    process.exit(0);
  }
  fs.mkdirSync(dir, { recursive: true });

  const pending: PendingAnnotation = {
    selectedText,
    context,
    capturedAt: new Date().toISOString(),
  };
  const pendingPath = normalizeWindowsPath(
    path.join(dir, `pending-${Date.now()}-${process.pid}.json`),
  );
  fs.writeFileSync(pendingPath, `${JSON.stringify(pending)}\n`, { mode: 0o600 });

  const opened = runHerdr([
    "plugin",
    "pane",
    "open",
    "--cwd",
    root,
    "--plugin",
    "annotate",
    "--entrypoint",
    "editor",
    "--placement",
    "popup",
    "--width",
    "88",
    "--height",
    "24",
    "--env",
    `HERDR_ANNOTATE_PENDING=${pendingPath}`,
    "--focus",
  ]);
  if (!opened.ok) {
    fs.rmSync(pendingPath, { force: true });
    throw new Error(opened.message);
  }
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  notify("Annotate failed", message);
  console.error(message);
  process.exit(1);
}
