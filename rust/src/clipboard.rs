//! Per-platform clipboard adapters.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardCommand {
    command: &'static str,
    args: &'static [&'static str],
}

// Windows PowerShell talks to a redirected pipe in the console's OEM code page (437 or 850), not
// UTF-8, so a selection like "Grão moído" comes back as "Grao mo\u{fffd}do" and text we write
// reaches the clipboard as mojibake. Reading pins stdout to UTF-8 without a BOM. Writing decodes
// the raw stdin bytes as UTF-8 itself, because setting [Console]::InputEncoding does not change
// how `$input` is decoded. Like `$input | Set-Clipboard` did, it drops one trailing line break and
// joins the lines with CRLF.
//
// Not `#[cfg(windows)]`: the `if cfg!(windows)` branches below are still compiled elsewhere, so
// the names must exist on every platform.
const WINDOWS_READ_SCRIPT: &str =
    "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; Get-Clipboard -Raw";
const WINDOWS_WRITE_SCRIPT: &str = "$m = New-Object System.IO.MemoryStream; [Console]::OpenStandardInput().CopyTo($m); $t = [System.Text.Encoding]::UTF8.GetString($m.ToArray()) -replace '\\r?\\n\\z', ''; if ($t) { Set-Clipboard -Value ($t -replace '(?<!\\r)\\n', [Environment]::NewLine) } else { Set-Clipboard -Value $null }";

fn read_commands() -> Vec<ClipboardCommand> {
    if cfg!(target_os = "macos") {
        return vec![ClipboardCommand {
            command: "pbpaste",
            args: &[],
        }];
    }
    if cfg!(windows) {
        return vec![ClipboardCommand {
            command: "powershell.exe",
            args: &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                WINDOWS_READ_SCRIPT,
            ],
        }];
    }
    vec![
        ClipboardCommand {
            command: "wl-paste",
            args: &["--no-newline"],
        },
        ClipboardCommand {
            command: "xclip",
            args: &["-selection", "clipboard", "-out"],
        },
        ClipboardCommand {
            command: "xsel",
            args: &["--clipboard", "--output"],
        },
    ]
}

fn write_commands() -> Vec<ClipboardCommand> {
    if cfg!(target_os = "macos") {
        return vec![ClipboardCommand {
            command: "pbcopy",
            args: &[],
        }];
    }
    if cfg!(windows) {
        return vec![ClipboardCommand {
            command: "powershell.exe",
            args: &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                WINDOWS_WRITE_SCRIPT,
            ],
        }];
    }
    vec![
        ClipboardCommand {
            command: "wl-copy",
            args: &[],
        },
        ClipboardCommand {
            command: "xclip",
            args: &["-selection", "clipboard", "-in"],
        },
        ClipboardCommand {
            command: "xsel",
            args: &["--clipboard", "--input"],
        },
    ]
}

fn process(command: &str) -> Command {
    let value = Command::new(command);
    #[cfg(windows)]
    let value = {
        use std::os::windows::process::CommandExt;
        let mut value = value;
        value.creation_flags(0x0800_0000);
        value
    };
    value
}

/// Read text from the first clipboard adapter available on the current platform.
pub fn read_clipboard() -> Result<String, String> {
    for candidate in read_commands() {
        let result = process(candidate.command)
            .args(candidate.args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(output) = result
            && output.status.success()
        {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    Err("No supported clipboard reader is available".to_owned())
}

/// Write text through the first clipboard adapter available on the current platform.
pub fn write_clipboard(text: &str) -> Result<(), String> {
    for candidate in write_commands() {
        let spawned = process(candidate.command)
            .args(candidate.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else { continue };
        let wrote = child
            .stdin
            .take()
            .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
        if wrote && child.wait().is_ok_and(|status| status.success()) {
            return Ok(());
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    Err("No supported clipboard writer is available".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_has_read_and_write_candidates() {
        assert_ne!(read_commands().len(), 0);
        assert_ne!(write_commands().len(), 0);
    }

    // These two drive the real system clipboard, so they are `#[ignore]`d. Run them on Windows
    // with `cargo test clipboard -- --ignored`. The other side of each check is a separate
    // PowerShell that moves UTF-16 code units as plain ASCII numbers, so the result does not
    // depend on any code page. Round-tripping `write_clipboard` into `read_clipboard` would not
    // catch this bug: the two code-page errors cancel out.
    //
    // The read test fails without the fix. The write test only fails when powershell.exe starts
    // with a console of its own, as it does when Herdr runs the plugin with no console attached;
    // from a terminal the child shares that terminal's console and can pass without the fix.
    #[cfg(windows)]
    mod windows_utf8 {
        #![allow(clippy::expect_used, reason = "tests assert by panicking")]
        use super::super::*;

        const TEXT: &str = "Grão moído, açúcar — ✓";

        fn powershell(script: &str) -> String {
            let output = process("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .stdin(Stdio::null())
                .output()
                .expect("run powershell.exe");
            assert!(
                output.status.success(),
                "powershell failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        }

        fn units() -> Vec<u16> {
            TEXT.encode_utf16().collect()
        }

        #[test]
        #[ignore = "overwrites the system clipboard"]
        fn reads_non_ascii_text_copied_by_another_program() {
            let list: Vec<String> = units().iter().map(u16::to_string).collect();
            powershell(&format!(
                "Set-Clipboard -Value (-join ([char[]]({})))",
                list.join(",")
            ));
            let read = read_clipboard().expect("read the clipboard");
            assert_eq!(read.trim_end(), TEXT);
        }

        #[test]
        #[ignore = "overwrites the system clipboard"]
        fn writes_non_ascii_text_other_programs_can_read() {
            write_clipboard(TEXT).expect("write the clipboard");
            let seen: Vec<u16> =
                powershell("(Get-Clipboard -Raw).ToCharArray() | ForEach-Object { [int]$_ }")
                    .lines()
                    .map(|line| line.trim().parse().expect("a UTF-16 code unit"))
                    .collect();
            assert_eq!(seen, units());
        }
    }
}
