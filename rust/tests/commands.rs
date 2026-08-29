//! Native command-boundary tests with a fake Herdr executable.

#![cfg(unix)]
#![allow(clippy::expect_used, reason = "tests assert by panicking")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn directory() -> PathBuf {
    let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "herdr-annotate-command-{}-{sequence}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary directory");
    dir
}

fn fake_herdr(dir: &Path) -> PathBuf {
    let script = dir.join("fake-herdr");
    fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HERDR_TEST_LOG\"\nexit \"${HERDR_TEST_EXIT:-0}\"\n",
    )
    .expect("fake Herdr");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).expect("executable");
    script
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_herdr-annotate"))
}

fn command(dir: &Path, subcommand: &str) -> (Command, PathBuf) {
    let log = dir.join("herdr.log");
    let mut command = Command::new(binary());
    command
        .arg(subcommand)
        .env("HERDR_BIN_PATH", fake_herdr(dir))
        .env("HERDR_TEST_LOG", &log)
        .env("HERDR_PLUGIN_STATE_DIR", dir.join("state"))
        .env("HERDR_PLUGIN_ROOT", dir.join("plugin"));
    (command, log)
}

#[test]
fn copy_context_with_an_empty_store_notifies_and_succeeds() {
    let dir = directory();
    let (mut command, log) = command(&dir, "copy-context");
    let output = command.output().expect("run");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(log).expect("notification"),
        "notification\nshow\nNo annotations\n--body\nThere is nothing to copy yet.\n"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn manage_opens_the_manager_pane_with_the_typescript_arguments() {
    let dir = directory();
    let root = dir.join("plugin");
    fs::create_dir_all(&root).expect("plugin root");
    let (mut command, log) = command(&dir, "manage");
    let output = command.output().expect("run");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(log).expect("Herdr call"),
        format!(
            "plugin\npane\nopen\n--cwd\n{}\n--plugin\nannotate\n--entrypoint\nmanager\n--placement\npopup\n--width\n100\n--height\n30\n--focus\n",
            root.display()
        )
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn capture_persists_selection_and_context_then_opens_the_editor() {
    let dir = directory();
    let root = dir.join("plugin");
    fs::create_dir_all(&root).expect("plugin root");
    let (mut command, log) = command(&dir, "capture");
    command.env(
        "HERDR_PLUGIN_CONTEXT_JSON",
        r#"{"selected_text":"  selected text\n","workspace_id":"workspace-1","tab_label":"server"}"#,
    );
    let output = command.output().expect("run");
    assert!(output.status.success(), "{output:?}");

    let pending = fs::read_dir(dir.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().starts_with("pending-"))
        .expect("pending file")
        .path();
    let value: Value =
        serde_json::from_str(&fs::read_to_string(&pending).expect("pending contents"))
            .expect("pending json");
    assert_eq!(
        value.get("selectedText").and_then(Value::as_str),
        Some("  selected text\n")
    );
    assert_eq!(
        value
            .pointer("/context/workspace_id")
            .and_then(Value::as_str),
        Some("workspace-1")
    );
    assert!(value.get("capturedAt").and_then(Value::as_str).is_some());

    let invocation = fs::read_to_string(log).expect("Herdr call");
    assert!(invocation.contains("plugin\npane\nopen\n"));
    assert!(invocation.contains("--entrypoint\neditor\n"));
    assert!(invocation.contains(&format!("HERDR_ANNOTATE_PENDING={}\n", pending.display())));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn failed_editor_open_removes_the_pending_file_and_reports_failure() {
    let dir = directory();
    let (mut command, _) = command(&dir, "capture");
    command
        .env(
            "HERDR_PLUGIN_CONTEXT_JSON",
            r#"{"selected_text":"selection"}"#,
        )
        .env("HERDR_TEST_EXIT", "1");
    let output = command.output().expect("run");
    assert!(!output.status.success());
    let pending_count = fs::read_dir(dir.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("pending-"))
        .count();
    assert_eq!(pending_count, 0);
    let _ = fs::remove_dir_all(dir);
}
