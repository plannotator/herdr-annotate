//! One native command boundary for the five Herdr entrypoints.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{SecondsFormat, Utc};
use serde_json::Value;

use crate::clipboard::{read_clipboard, write_clipboard};
use crate::format::format_annotations;
use crate::handoff::take_default_handoff;
use crate::herdr::{notify, run_herdr};
use crate::paths::{normalize_windows_path, plugin_root, state_dir};
use crate::store::{load_annotations, newest_first_annotations};
use crate::types::{
    PendingAnnotation, javascript_trim, parse_invocation_context, selected_text_from_invocation,
};

const USAGE: &str = "Usage: herdr-annotate <capture|copy-context|editor|manage|manager>";

/// Dispatch one native binary subcommand.
pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("capture") if args.len() == 1 => capture().inspect_err(|message| {
            notify("Annotate failed", Some(message));
        }),
        Some("copy-context") if args.len() == 1 => copy_context().inspect_err(|message| {
            notify("Copy failed", Some(message));
        }),
        Some("manage") if args.len() == 1 => manage().inspect_err(|message| {
            notify("Unable to open annotations", Some(message));
        }),
        Some("editor") if args.len() == 1 => crate::editor::run(),
        Some("manager") if args.len() == 1 => crate::manager::run(),
        Some("--version" | "-V") if args.len() == 1 => {
            #[allow(clippy::print_stdout, reason = "the version command prints its result")]
            {
                println!("herdr-annotate {}", env!("CARGO_PKG_VERSION"));
            }
            Ok(())
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn invocation_context() -> Value {
    std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

fn capture() -> Result<(), String> {
    let decoded = invocation_context();
    let context = parse_invocation_context(&decoded);
    let mut selected_text = selected_text_from_invocation(&decoded);
    let dir = state_dir().ok_or_else(|| "HERDR_PLUGIN_STATE_DIR is not set".to_owned())?;
    let root = plugin_root().ok_or_else(|| "HERDR_PLUGIN_ROOT is not set".to_owned())?;
    if selected_text.is_none() {
        selected_text = take_default_handoff()?;
    }
    if selected_text.is_none() {
        selected_text = Some(read_clipboard()?);
    }
    let selected_text = selected_text.unwrap_or_default();
    if javascript_trim(&selected_text).is_empty() {
        notify(
            "Nothing to annotate",
            Some("Select text in Herdr or copy text to the clipboard."),
        );
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let pending = PendingAnnotation {
        selected_text,
        context,
        captured_at: now_iso(),
    };
    let millis = unix_millis();
    let raw_path = dir.join(format!("pending-{millis}-{}.json", std::process::id()));
    let pending_path =
        std::path::PathBuf::from(normalize_windows_path(&raw_path.to_string_lossy()));
    write_pending(&pending_path, &pending)?;

    let opened = run_herdr(&[
        "plugin".to_owned(),
        "pane".to_owned(),
        "open".to_owned(),
        "--cwd".to_owned(),
        root.to_string_lossy().into_owned(),
        "--plugin".to_owned(),
        "annotate".to_owned(),
        "--entrypoint".to_owned(),
        "editor".to_owned(),
        "--placement".to_owned(),
        "popup".to_owned(),
        "--width".to_owned(),
        "88".to_owned(),
        "--height".to_owned(),
        "24".to_owned(),
        "--env".to_owned(),
        format!("HERDR_ANNOTATE_PENDING={}", pending_path.display()),
        "--focus".to_owned(),
    ]);
    if let Err(message) = opened {
        let _ = std::fs::remove_file(pending_path);
        return Err(message);
    }
    Ok(())
}

fn copy_context() -> Result<(), String> {
    let dir = state_dir().ok_or_else(|| "HERDR_PLUGIN_STATE_DIR is not set".to_owned())?;
    let annotations = newest_first_annotations(&load_annotations(&dir)?);
    if annotations.is_empty() {
        notify("No annotations", Some("There is nothing to copy yet."));
        return Ok(());
    }
    write_clipboard(&format_annotations(&annotations))?;
    notify(
        "Annotations copied",
        Some(&format!(
            "{} annotation{} copied as Markdown.",
            annotations.len(),
            if annotations.len() == 1 { "" } else { "s" }
        )),
    );
    Ok(())
}

fn manage() -> Result<(), String> {
    let root = plugin_root().ok_or_else(|| "HERDR_PLUGIN_ROOT is not set".to_owned())?;
    run_herdr(&[
        "plugin".to_owned(),
        "pane".to_owned(),
        "open".to_owned(),
        "--cwd".to_owned(),
        root.to_string_lossy().into_owned(),
        "--plugin".to_owned(),
        "annotate".to_owned(),
        "--entrypoint".to_owned(),
        "manager".to_owned(),
        "--placement".to_owned(),
        "popup".to_owned(),
        "--width".to_owned(),
        "100".to_owned(),
        "--height".to_owned(),
        "30".to_owned(),
        "--focus".to_owned(),
    ])
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_pending(path: &Path, pending: &PendingAnnotation) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut file, pending).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_extra_arguments_report_the_single_binary_usage() {
        assert_eq!(run(&[]), Err(USAGE.to_owned()));
        assert_eq!(run(&["unknown".to_owned()]), Err(USAGE.to_owned()));
        assert_eq!(
            run(&["capture".to_owned(), "extra".to_owned()]),
            Err(USAGE.to_owned())
        );
    }
}
