import { afterAll, describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = path.resolve(import.meta.dir, "..");
const binary = path.join(root, "bin", "herdr-annotate.exe");
const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "herdr-annotate-runtime-"));

afterAll(() => fs.rmSync(temporaryRoot, { recursive: true, force: true }));

describe("compiled plugin runtime", () => {
  test("runs an action when Bun is absent from PATH", () => {
    const built = spawnSync(process.execPath, ["run", "scripts/build-plugin.ts"], {
      cwd: root,
      encoding: "utf8",
    });
    expect(built.status, built.stderr).toBe(0);

    const emptyPath = path.join(temporaryRoot, "empty-path");
    const state = path.join(temporaryRoot, "state");
    fs.mkdirSync(emptyPath);

    const runtime = spawnSync(binary, ["copy-context"], {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: emptyPath,
        HERDR_BIN_PATH: process.execPath,
        HERDR_PLUGIN_STATE_DIR: state,
      },
    });

    expect(runtime.status, runtime.stderr).toBe(0);
  });
});
