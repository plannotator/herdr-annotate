import path from "node:path";

/** Remove Windows extended-path prefixes that Bun and CreateProcess cannot use as a cwd. */
export function normalizeWindowsPath(value: string): string;
export function normalizeWindowsPath(value: undefined): undefined;
export function normalizeWindowsPath(value: string | undefined): string | undefined {
  if (!value) return value;

  if (value.startsWith("\\\\?\\")) {
    const withoutPrefix = value.slice(4);
    return withoutPrefix.slice(0, 4).toUpperCase() === "UNC\\"
      ? `\\\\${withoutPrefix.slice(4)}`
      : withoutPrefix;
  }

  if (value.startsWith("//?/")) {
    const withoutPrefix = value.slice(4);
    return withoutPrefix.slice(0, 4).toUpperCase() === "UNC/"
      ? `//${withoutPrefix.slice(4)}`
      : withoutPrefix;
  }

  return value;
}

/** Return Herdr's plugin-owned state directory when the runtime supplied one. */
export function stateDir(): string | undefined {
  return process.env.HERDR_PLUGIN_STATE_DIR || undefined;
}

/** Return Herdr's plugin root in a form that Bun and CreateProcess can use as a cwd. */
export function pluginRoot(): string | undefined {
  const value = process.env.HERDR_PLUGIN_ROOT;
  return value ? normalizeWindowsPath(value) : undefined;
}

/** Resolve the JSONL store inside an already parsed plugin state directory. */
export function annotationsPath(dir: string): string {
  return path.join(dir, "annotations.jsonl");
}

/** Resolve the archived-set JSONL store inside the plugin state directory. */
export function archivesPath(dir: string): string {
  return path.join(dir, "archives.jsonl");
}
