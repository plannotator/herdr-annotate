#!/usr/bin/env bun
import { notify, runHerdr } from "./herdr";
import { pluginRoot } from "./paths";

const root = pluginRoot();
if (!root) {
  const message = "HERDR_PLUGIN_ROOT is not set";
  notify("Unable to open annotations", message);
  console.error(message);
  process.exit(1);
}

const opened = runHerdr([
  "plugin",
  "pane",
  "open",
  "--cwd",
  root,
  "--plugin",
  "annotate",
  "--entrypoint",
  "manager",
  "--placement",
  "popup",
  "--width",
  "100",
  "--height",
  "30",
  "--focus",
]);

if (!opened.ok) {
  notify("Unable to open annotations", opened.message);
  console.error(opened.message);
  process.exit(1);
}
