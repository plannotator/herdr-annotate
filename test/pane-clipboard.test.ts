import { describe, expect, test } from "bun:test";
import {
  OSC52_COMMON_PAYLOAD_LIMIT_BYTES,
  exceedsCommonOsc52Limit,
  osc52ClipboardSequence,
  paneClipboardWriter,
} from "../src/pane-clipboard";

describe("OSC 52 encoding", () => {
  test("emits the exact bytes for plain text", () => {
    const sequence = osc52ClipboardSequence("hi");

    expect(sequence).toBe("\x1b]52;c;aGk=\x07");
    expect([...Buffer.from(sequence, "utf8")]).toEqual([
      0x1b, 0x5d, 0x35, 0x32, 0x3b, 0x63, 0x3b, 0x61, 0x47, 0x6b, 0x3d, 0x07,
    ]);
  });

  test("emits an empty payload for an empty string", () => {
    expect(osc52ClipboardSequence("")).toBe("\x1b]52;c;\x07");
  });

  test("encodes the raw UTF-8 bytes of multi-byte text", () => {
    expect(osc52ClipboardSequence("한글 · é")).toBe("\x1b]52;c;7ZWc6riAIMK3IMOp\x07");
    expect(osc52ClipboardSequence("한글 · é")).toBe(
      `\x1b]52;c;${Buffer.from("한글 · é", "utf8").toString("base64")}\x07`,
    );
  });

  test("reports oversized payloads without truncating them", () => {
    const text = "a".repeat(OSC52_COMMON_PAYLOAD_LIMIT_BYTES);

    expect(exceedsCommonOsc52Limit("hi")).toBe(false);
    expect(exceedsCommonOsc52Limit(text)).toBe(true);
    expect(osc52ClipboardSequence(text)).toContain(Buffer.from(text, "utf8").toString("base64"));
  });
});

describe("pane clipboard writer", () => {
  test("emits the sequence after a successful native write", () => {
    const order: string[] = [];
    const write = paneClipboardWriter(
      (text) => {
        order.push(`native:${text}`);
        return { ok: true, value: undefined };
      },
      (sequence) => {
        order.push(`emit:${sequence}`);
        return true;
      },
    );

    expect(write("hi")).toEqual({ ok: true, value: undefined });
    expect(order).toEqual(["native:hi", "emit:\x1b]52;c;aGk=\x07"]);
  });

  test("a copy the terminal received succeeds even when the native write failed", () => {
    const emitted: string[] = [];
    const write = paneClipboardWriter(
      () => ({ ok: false, message: "No supported clipboard writer is available" }),
      (sequence) => {
        emitted.push(sequence);
        return true;
      },
    );

    expect(write("hi")).toEqual({ ok: true, value: undefined });
    expect(emitted).toEqual(["\x1b]52;c;aGk=\x07"]);
  });

  test("a copy that reached neither destination fails with the native error", () => {
    const write = paneClipboardWriter(
      () => ({ ok: false, message: "No supported clipboard writer is available" }),
      () => false,
    );

    expect(write("hi")).toEqual({
      ok: false,
      message: "No supported clipboard writer is available",
    });
  });
});
