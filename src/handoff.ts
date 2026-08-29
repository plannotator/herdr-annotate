import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/**
 * A selection handed to `annotate.capture` by another program on the same machine as the
 * Herdr server (Neovim, a script), for hosts where the plugin cannot read a clipboard:
 * headless servers reached over SSH or `herdr --remote`.
 *
 * The writer puts the text in {@link handoffPath} and invokes the action; the action takes
 * the file (reads and deletes it) when it is fresh. Anything older is a leftover, not a
 * selection, and is ignored so a stale file can never be annotated by surprise.
 */
export const HANDOFF_MAX_AGE_MS = 15_000;

/** `$XDG_RUNTIME_DIR` (per user, tmpfs) when set, else the system temp dir, plus the uid. */
export function handoffPath(env: NodeJS.ProcessEnv = process.env): string {
  const base = env.XDG_RUNTIME_DIR?.trim() || os.tmpdir();
  const uid = typeof process.getuid === "function" ? String(process.getuid()) : "user";
  return path.join(base, `herdr-annotate-${uid}`, "selection");
}

/** The handed-off text when the file exists and is fresh; the file is removed either way. */
export function takeHandoff(
  file: string = handoffPath(),
  now: number = Date.now(),
  maxAgeMs: number = HANDOFF_MAX_AGE_MS,
): string | undefined {
  let stat: fs.Stats;
  try {
    stat = fs.statSync(file);
  } catch {
    return undefined;
  }
  let text: string | undefined;
  if (stat.isFile() && now - stat.mtimeMs <= maxAgeMs) {
    try {
      text = fs.readFileSync(file, "utf8");
    } catch {
      text = undefined;
    }
  }
  fs.rmSync(file, { force: true });
  return text && text.trim() ? text : undefined;
}
