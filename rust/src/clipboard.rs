//! Per-platform clipboard adapters.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardCommand {
    command: &'static str,
    args: &'static [&'static str],
}

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
                "Get-Clipboard -Raw",
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
                "$input | Set-Clipboard",
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
        assert!(!read_commands().is_empty());
        assert!(!write_commands().is_empty());
    }
}
