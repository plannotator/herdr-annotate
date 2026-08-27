import { spawnSync } from "node:child_process";

interface ClipboardCommand {
  command: string;
  args: string[];
}

/** The result of an expected clipboard operation. */
export type ClipboardResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };

function readCommands(): ClipboardCommand[] {
  if (process.platform === "darwin") return [{ command: "pbpaste", args: [] }];
  if (process.platform === "win32") {
    return [
      {
        command: "powershell.exe",
        args: ["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard -Raw"],
      },
    ];
  }
  return [
    { command: "wl-paste", args: ["--no-newline"] },
    { command: "xclip", args: ["-selection", "clipboard", "-out"] },
    { command: "xsel", args: ["--clipboard", "--output"] },
  ];
}

function writeCommands(): ClipboardCommand[] {
  if (process.platform === "darwin") return [{ command: "pbcopy", args: [] }];
  if (process.platform === "win32") {
    return [
      {
        command: "powershell.exe",
        args: ["-NoProfile", "-NonInteractive", "-Command", "$input | Set-Clipboard"],
      },
    ];
  }
  return [
    { command: "wl-copy", args: [] },
    { command: "xclip", args: ["-selection", "clipboard", "-in"] },
    { command: "xsel", args: ["--clipboard", "--input"] },
  ];
}

/** Read text from the first clipboard adapter available on the current platform. */
export function readClipboard(): ClipboardResult<string> {
  for (const candidate of readCommands()) {
    const result = spawnSync(candidate.command, candidate.args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      windowsHide: process.platform === "win32",
    });
    if (result.status === 0 && typeof result.stdout === "string") {
      return { ok: true, value: result.stdout };
    }
  }
  return { ok: false, message: "No supported clipboard reader is available" };
}

/** Write text through the first clipboard adapter available on the current platform. */
export function writeClipboard(text: string): ClipboardResult<undefined> {
  for (const candidate of writeCommands()) {
    const result = spawnSync(candidate.command, candidate.args, {
      input: text,
      encoding: "utf8",
      stdio: ["pipe", "ignore", "ignore"],
      windowsHide: process.platform === "win32",
    });
    if (result.status === 0) return { ok: true, value: undefined };
  }
  return { ok: false, message: "No supported clipboard writer is available" };
}
