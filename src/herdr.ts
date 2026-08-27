import { spawnSync } from "node:child_process";

const binary = process.env.HERDR_BIN_PATH ?? "herdr";

/** The result of invoking Herdr for an expected plugin operation. */
export type HerdrResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly message: string };

/** Invoke Herdr synchronously and return a safe error projection on failure. */
export function runHerdr(args: readonly string[]): HerdrResult {
  const result = spawnSync(binary, args, {
    encoding: "utf8",
    stdio: ["ignore", "ignore", "pipe"],
    windowsHide: process.platform === "win32",
  });
  if (result.status !== 0) {
    return {
      ok: false,
      message: result.stderr?.trim() || `herdr ${args.join(" ")} failed`,
    };
  }
  return { ok: true };
}

/** Best-effort user notification; failures are intentionally non-fatal. */
export function notify(title: string, body?: string): void {
  const args = ["notification", "show", title];
  if (body) args.push("--body", body);
  spawnSync(binary, args, {
    encoding: "utf8",
    stdio: ["ignore", "ignore", "pipe"],
    windowsHide: process.platform === "win32",
  });
}
