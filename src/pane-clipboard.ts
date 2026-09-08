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
 * Wrap a native clipboard writer so every pane copy also reaches the viewing client's terminal.
 *
 * The native write is attempted first and the sequence is emitted afterwards, so a server without a
 * clipboard writer still delivers the copy. When only the sequence lands the result reports the copy
 * that succeeded and keeps the native failure visible instead of hiding it behind a bare success.
 */
export function paneClipboardWriter(
  writeClipboard: (text: string) => ClipboardResult<undefined>,
  emit: (sequence: string) => void,
): (text: string) => ClipboardResult<undefined> {
  return (text: string) => {
    const native = writeClipboard(text);
    emit(osc52ClipboardSequence(text));
    if (native.ok) return native;
    return { ok: false, message: `Copied to this terminal. Server clipboard: ${native.message}` };
  };
}
