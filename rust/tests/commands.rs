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

/// A fake Herdr that appends every call, answers `agent get` with `HERDR_TEST_AGENT_GET`, and
/// fails the one command named in `HERDR_TEST_FAIL` with `HERDR_TEST_STDERR`.
fn responding_herdr(dir: &Path) -> PathBuf {
    let script = dir.join("responding-herdr");
    fs::write(
        &script,
        r#"#!/bin/sh
printf '%s\n' "$@" >> "$HERDR_TEST_LOG"
printf '%s\n' '--' >> "$HERDR_TEST_LOG"
if [ "$1 $2" = "${HERDR_TEST_FAIL:-}" ]; then
  printf '%s\n' "$HERDR_TEST_STDERR" >&2
  exit 1
fi
if [ "$1 $2" = "agent get" ]; then
  printf '%s\n' "${HERDR_TEST_AGENT_GET:-}"
fi
exit 0
"#,
    )
    .expect("fake Herdr");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).expect("executable");
    script
}

const SEEDED: &str = concat!(
    r#"{"selectedText":"first selection","capturedAt":"2026-09-24T00:00:00.000Z","context":{},"#,
    r#""id":"one","comment":"first comment","createdAt":"2026-09-24T00:00:01.000Z"}"#,
    "\n",
    r#"{"selectedText":"second selection","capturedAt":"2026-09-24T00:00:02.000Z","context":{},"#,
    r#""id":"two","comment":"second comment","createdAt":"2026-09-24T00:00:03.000Z"}"#,
    "\n",
);

const FOCUSED: &str = r#"{"focused_pane_id":"w1:p2"}"#;

fn agent_record(status: &str) -> String {
    format!(
        r#"{{"id":"cli:agent:get","result":{{"agent":{{"agent":"claude","agent_status":"{status}","pane_id":"w1:p2"}},"type":"agent_info"}}}}"#
    )
}

/// Run one delivery subcommand against a seeded store, returning its output, calls and state.
fn deliver(
    subcommand: &str,
    context: Option<&str>,
    variables: &[(&str, &str)],
) -> (std::process::Output, String, PathBuf) {
    let dir = directory();
    let state = dir.join("state");
    fs::create_dir_all(&state).expect("state");
    fs::write(state.join("annotations.jsonl"), SEEDED).expect("seed");
    let log = dir.join("herdr.log");
    let mut command = Command::new(binary());
    command
        .arg(subcommand)
        .env("HERDR_BIN_PATH", responding_herdr(&dir))
        .env("HERDR_TEST_LOG", &log)
        .env("HERDR_PLUGIN_STATE_DIR", &state)
        .env_remove("HERDR_PLUGIN_CONTEXT_JSON");
    if let Some(context) = context {
        command.env("HERDR_PLUGIN_CONTEXT_JSON", context);
    }
    command.envs(variables.iter().copied());
    let output = command.output().expect("run");
    let calls = fs::read_to_string(&log).unwrap_or_default();
    (output, calls, state)
}

fn active(state: &Path) -> String {
    fs::read_to_string(state.join("annotations.jsonl")).expect("active store")
}

fn archived(state: &Path) -> usize {
    fs::read_to_string(state.join("archives.jsonl"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn remove(state: &Path) {
    if let Some(dir) = state.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn send_archive_prompts_the_focused_agent_then_archives() {
    let record = agent_record("idle");
    let (output, calls, state) = deliver(
        "send-archive",
        Some(FOCUSED),
        &[("HERDR_TEST_AGENT_GET", &record)],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        calls.starts_with(
            "agent\nget\nw1:p2\n--\nagent\nprompt\nw1:p2\n# Annotated context\n\n## Annotation 1\n"
        ),
        "{calls}"
    );
    assert!(calls.find("second selection") < calls.find("first selection"));
    assert!(!calls.contains('\x1b'), "{calls}");
    assert!(calls.ends_with(
        "--\nnotification\nshow\nAnnotations sent and archived\n--body\n\
         2 annotations sent to the agent and archived.\n--\n"
    ));
    assert_eq!(active(&state), "");
    assert_eq!(archived(&state), 1);
    remove(&state);
}

#[test]
fn paste_archive_checks_the_agent_then_types_one_bracketed_paste() {
    let record = agent_record("idle");
    let (output, calls, state) = deliver(
        "paste-archive",
        Some(FOCUSED),
        &[("HERDR_TEST_AGENT_GET", &record)],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        calls.starts_with(
            "agent\nget\nw1:p2\n--\npane\nsend-text\nw1:p2\n\x1b[200~# Annotated context\n"
        ),
        "{calls}"
    );
    assert!(calls.contains(
        "first comment\n\x1b[201~\n--\nnotification\nshow\nAnnotations pasted and archived\n"
    ));
    assert_eq!(calls.matches('\x1b').count(), 2);
    assert_eq!(active(&state), "");
    assert_eq!(archived(&state), 1);
    remove(&state);
}

#[test]
fn paste_archive_refuses_a_blocked_agent_and_keeps_the_store() {
    let record = agent_record("blocked");
    let (output, calls, state) = deliver(
        "paste-archive",
        Some(FOCUSED),
        &[("HERDR_TEST_AGENT_GET", &record)],
    );
    assert!(!output.status.success());
    assert_eq!(
        calls,
        "agent\nget\nw1:p2\n--\nnotification\nshow\nPaste failed\n--body\n\
         The agent is waiting on a prompt. Nothing was pasted; your annotations are still active.\n--\n"
    );
    assert_eq!(active(&state), SEEDED);
    assert_eq!(archived(&state), 0);
    remove(&state);
}

#[test]
fn send_archive_turns_herdrs_refusal_into_words_and_keeps_the_store() {
    let record = agent_record("idle");
    let (output, calls, state) = deliver(
        "send-archive",
        Some(FOCUSED),
        &[
            ("HERDR_TEST_AGENT_GET", &record),
            ("HERDR_TEST_FAIL", "agent prompt"),
            (
                "HERDR_TEST_STDERR",
                r#"{"error":{"code":"agent_not_found","message":"agent target w1:p2 not found"},"id":"cli:agent:prompt"}"#,
            ),
        ],
    );
    assert!(!output.status.success());
    assert!(
        calls.ends_with(
            "--\nnotification\nshow\nSend failed\n--body\n\
             No agent is running in the focused pane. Nothing was sent; your annotations are still active.\n--\n"
        ),
        "{calls}"
    );
    assert_eq!(active(&state), SEEDED);
    assert_eq!(archived(&state), 0);
    remove(&state);
}

/// Herdr before 0.8.2 types `agent prompt` text into an approval dialog, so send-archive must
/// refuse on the `agent get` answer and never reach `agent prompt`.
#[test]
fn send_archive_refuses_an_agent_that_is_not_ready_without_prompting() {
    let blocked = agent_record("blocked");
    let exited = r#"{"id":"cli:agent:get","result":{"agent":{"name":"reviewer","agent_status":"idle","pane_id":"w1:p2"},"type":"agent_info"}}"#;
    let launching = r#"{"id":"cli:agent:get","result":{"agent":{"agent":"claude","agent_status":"unknown","launch_pending":true,"pane_id":"w1:p2"},"type":"agent_info"}}"#;
    for (record, reason) in [
        (blocked.as_str(), "The agent is waiting on a prompt."),
        (exited, "No agent is running in the focused pane."),
        (launching, "The agent is not ready for input yet."),
    ] {
        let (output, calls, state) = deliver(
            "send-archive",
            Some(FOCUSED),
            &[("HERDR_TEST_AGENT_GET", record)],
        );
        assert!(!output.status.success(), "{reason}");
        assert_eq!(
            calls,
            format!(
                "agent\nget\nw1:p2\n--\nnotification\nshow\nSend failed\n--body\n\
                 {reason} Nothing was sent; your annotations are still active.\n--\n"
            )
        );
        assert_eq!(active(&state), SEEDED);
        assert_eq!(archived(&state), 0);
        remove(&state);
    }
}

#[test]
fn paste_archive_refuses_a_pane_whose_agent_exited() {
    let exited = r#"{"id":"cli:agent:get","result":{"agent":{"name":"reviewer","agent_status":"unknown","pane_id":"w1:p2"},"type":"agent_info"}}"#;
    let (output, calls, state) = deliver(
        "paste-archive",
        Some(FOCUSED),
        &[("HERDR_TEST_AGENT_GET", exited)],
    );
    assert!(!output.status.success());
    assert_eq!(
        calls,
        "agent\nget\nw1:p2\n--\nnotification\nshow\nPaste failed\n--body\n\
         No agent is running in the focused pane. Nothing was pasted; your annotations are still active.\n--\n"
    );
    assert_eq!(active(&state), SEEDED);
    assert_eq!(archived(&state), 0);
    remove(&state);
}

#[test]
fn delivery_without_a_focused_pane_is_refused_before_calling_the_agent() {
    for (subcommand, title, past) in [
        ("paste-archive", "Paste failed", "pasted"),
        ("send-archive", "Send failed", "sent"),
    ] {
        let (output, calls, state) = deliver(subcommand, None, &[]);
        assert!(!output.status.success());
        assert_eq!(
            calls,
            format!(
                "notification\nshow\n{title}\n--body\nHerdr did not say which pane is focused. \
                 Nothing was {past}; your annotations are still active.\n--\n"
            )
        );
        assert_eq!(active(&state), SEEDED);
        remove(&state);
    }
}

#[test]
fn delivery_with_an_empty_store_says_there_is_nothing_to_deliver() {
    for (subcommand, verb) in [("paste-archive", "paste"), ("send-archive", "send")] {
        let dir = directory();
        let log = dir.join("herdr.log");
        let output = Command::new(binary())
            .arg(subcommand)
            .env("HERDR_BIN_PATH", responding_herdr(&dir))
            .env("HERDR_TEST_LOG", &log)
            .env("HERDR_PLUGIN_STATE_DIR", dir.join("state"))
            .env("HERDR_PLUGIN_CONTEXT_JSON", FOCUSED)
            .output()
            .expect("run");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            fs::read_to_string(log).expect("notification"),
            format!(
                "notification\nshow\nNo annotations\n--body\nThere is nothing to {verb} yet.\n--\n"
            )
        );
        let _ = fs::remove_dir_all(dir);
    }
}
