/**
 * Clipboard writes performed from inside a Herdr pane.
 *
 * Herdr 0.9.0 forwards OSC 52 sequences emitted by pane output to the viewing client, so a copy made
 * inside a pane can reach the clipboard of the machine the person is sitting at rather than only the
 * machine the plugin runs on. Panes emit the sequence in addition to the native clipboard write; the
 * global actions run with piped stdout and no terminal, so they cannot use this path.
 */
import type { ClipboardResult } from "./clipboard";

/**
 * Base64 payload size that terminals commonly refuse beyond. Herdr forwards whatever the terminal
 * accepts, so this is advisory only: oversized text is still emitted in full, never truncated.
 */
export const OSC52_COMMON_PAYLOAD_LIMIT_BYTES = 74994;

/** Build the OSC 52 sequence that sets the terminal clipboard selection to `text`. */
export function osc52ClipboardSequence(text: string): string {
  return `\x1b]52;c;${Buffer.from(text, "utf8").toString("base64")}\x07`;
}

/** Whether the encoded payload is larger than terminals commonly accept. */
export function exceedsCommonOsc52Limit(text: string): boolean {
  return Buffer.from(text, "utf8").toString("base64").length > OSC52_COMMON_PAYLOAD_LIMIT_BYTES;
}

/**
 * Write the sequence to the pane's terminal, reporting whether it was written.
 *
 * Best effort: a closed or broken stdout must not throw out of a copy.
 */
export function emitToTerminal(sequence: string): boolean {
  try {
    process.stdout.write(sequence);
    return true;
  } catch {
    return false;
  }
}

/**
 * Wrap a native clipboard writer so every pane copy also reaches the viewing client's terminal.
 *
 * The native write is attempted first and the sequence is emitted afterwards. Either one landing is
 * a successful copy: a remote server commonly has no clipboard tool at all, and the sequence is the
 * copy that actually reached the person, so it must not be reported as a failure or block the rest
 * of a copy-and-archive. Only a copy that reached neither destination fails, with the native error.
 */
export function paneClipboardWriter(
  writeClipboard: (text: string) => ClipboardResult<undefined>,
  emit: (sequence: string) => boolean,
): (text: string) => ClipboardResult<undefined> {
  return (text: string) => {
    const native = writeClipboard(text);
    const emitted = emit(osc52ClipboardSequence(text));
    if (native.ok || emitted) return { ok: true, value: undefined };
    return native;
  };
}
