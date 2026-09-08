//! Clipboard writes performed from inside a Herdr pane.
//!
//! Herdr 0.9.0 forwards OSC 52 sequences emitted by pane output to the viewing client, so a copy
//! made inside a pane can reach the clipboard of the machine the person is sitting at rather than
//! only the machine the plugin runs on. Panes emit the sequence in addition to the native clipboard
//! write; the global actions run with piped stdout and no terminal, so they cannot use this path.

use std::io::Write;

/// Base64 payload size that terminals commonly refuse beyond. Herdr forwards whatever the terminal
/// accepts, so this is advisory only: oversized text is still emitted in full, never truncated.
pub const OSC52_COMMON_PAYLOAD_LIMIT_BYTES: usize = 74_994;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk.first().copied().unwrap_or(0));
        let second = u32::from(chunk.get(1).copied().unwrap_or(0));
        let third = u32::from(chunk.get(2).copied().unwrap_or(0));
        let value = (first << 16) | (second << 8) | third;
        let indexes = [
            (value >> 18) & 0x3F,
            (value >> 12) & 0x3F,
            (value >> 6) & 0x3F,
            value & 0x3F,
        ];
        for (offset, index) in indexes.into_iter().enumerate() {
            if offset > chunk.len() {
                encoded.push('=');
            } else if let Some(symbol) = BASE64_ALPHABET.get(index as usize) {
                encoded.push(char::from(*symbol));
            }
        }
    }
    encoded
}

/// Build the OSC 52 sequence that sets the terminal clipboard selection to `text`.
pub fn osc52_clipboard_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Whether the encoded payload is larger than terminals commonly accept.
pub fn exceeds_common_osc52_limit(text: &str) -> bool {
    base64(text.as_bytes()).len() > OSC52_COMMON_PAYLOAD_LIMIT_BYTES
}

/// Write the OSC 52 sequence to the pane's terminal.
///
/// Emission is best effort: a terminal that drops the sequence must not fail the copy.
pub fn emit_to_terminal(sequence: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(sequence.as_bytes());
    let _ = stdout.flush();
}

/// Perform a pane clipboard write: the native write first, then the OSC 52 sequence.
///
/// When only the sequence lands the result reports the copy that succeeded and keeps the native
/// failure visible instead of hiding it behind a bare success.
pub fn write_pane_clipboard(
    text: &str,
    write_clipboard: impl FnOnce(&str) -> Result<(), String>,
    emit: impl FnOnce(&str),
) -> Result<(), String> {
    let native = write_clipboard(text);
    emit(&osc52_clipboard_sequence(text));
    native.map_err(|message| format!("Copied to this terminal. Server clipboard: {message}"))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn plain_text_encodes_to_the_exact_osc52_bytes() {
        let sequence = osc52_clipboard_sequence("hi");
        assert_eq!(sequence, "\x1b]52;c;aGk=\x07");
        assert_eq!(
            sequence.as_bytes(),
            &[
                0x1b, 0x5d, 0x35, 0x32, 0x3b, 0x63, 0x3b, 0x61, 0x47, 0x6b, 0x3d, 0x07
            ]
        );
    }

    #[test]
    fn an_empty_string_encodes_to_an_empty_payload() {
        assert_eq!(osc52_clipboard_sequence(""), "\x1b]52;c;\x07");
    }

    #[test]
    fn multi_byte_text_encodes_its_raw_utf8_bytes() {
        assert_eq!(
            osc52_clipboard_sequence("한글 · é"),
            "\x1b]52;c;7ZWc6riAIMK3IMOp\x07"
        );
    }

    #[test]
    fn base64_pads_every_chunk_length() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"abcd"), "YWJjZA==");
        assert_eq!(base64(&[0xFF, 0xFE, 0xFD]), "//79");
    }

    #[test]
    fn oversized_payloads_are_reported_without_truncation() {
        let text = "a".repeat(OSC52_COMMON_PAYLOAD_LIMIT_BYTES);
        assert!(!exceeds_common_osc52_limit("hi"));
        assert!(exceeds_common_osc52_limit(&text));
        assert!(osc52_clipboard_sequence(&text).contains(&base64(text.as_bytes())));
    }

    #[test]
    fn a_successful_native_write_emits_the_sequence_afterwards() {
        let order = RefCell::new(Vec::new());
        let result = write_pane_clipboard(
            "hi",
            |text| {
                order.borrow_mut().push(format!("native:{text}"));
                Ok(())
            },
            |sequence| order.borrow_mut().push(format!("emit:{sequence}")),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            *order.borrow(),
            vec!["native:hi".to_owned(), "emit:\x1b]52;c;aGk=\x07".to_owned()]
        );
    }

    #[test]
    fn a_failed_native_write_still_emits_and_reports_the_terminal_copy() {
        let emitted = RefCell::new(Vec::new());
        let result = write_pane_clipboard(
            "hi",
            |_| Err("No supported clipboard writer is available".to_owned()),
            |sequence| emitted.borrow_mut().push(sequence.to_owned()),
        );
        assert_eq!(
            result,
            Err(
                "Copied to this terminal. Server clipboard: No supported clipboard writer is available"
                    .to_owned()
            )
        );
        assert_eq!(*emitted.borrow(), vec!["\x1b]52;c;aGk=\x07".to_owned()]);
    }
}
