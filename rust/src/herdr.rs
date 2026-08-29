//! Calls back into the Herdr CLI.

use std::process::{Command, Stdio};

fn binary() -> std::ffi::OsString {
    std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into())
}

fn process() -> Command {
    let value = Command::new(binary());
    #[cfg(windows)]
    let value = {
        use std::os::windows::process::CommandExt;
        let mut value = value;
        value.creation_flags(0x0800_0000);
        value
    };
    value
}

/// Invoke Herdr synchronously and return a safe error projection on failure.
pub fn run_herdr(args: &[String]) -> Result<(), String> {
    let result = process()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if stderr.is_empty() {
                Err(format!("herdr {} failed", args.join(" ")))
            } else {
                Err(stderr)
            }
        }
        Err(_) => Err(format!("herdr {} failed", args.join(" "))),
    }
}

/// Best-effort user notification; failures are intentionally non-fatal.
pub fn notify(title: &str, body: Option<&str>) {
    let mut args = vec!["notification", "show", title];
    if let Some(body) = body {
        args.extend(["--body", body]);
    }
    let _ = process()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
