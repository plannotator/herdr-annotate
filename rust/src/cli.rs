//! One native command boundary for the eight Herdr entrypoints.

use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::agent_delivery::{Delivery, deliver_to_agent};
use crate::archive_workflow::{
    CopyAndArchiveDependencies, CopyAndArchiveOutcome, copy_and_archive_annotations,
};
use crate::clipboard::{read_clipboard, write_clipboard};
use crate::format::format_annotations;
use crate::handoff::take_default_handoff;
use crate::herdr::{notify, run_herdr, run_herdr_output};
use crate::paths::{normalize_windows_path, plugin_root, state_dir};
use crate::store::{
    append_archived_set, load_annotations, newest_first_annotations, remove_annotations_by_id,
};
use crate::types::{
    ArchivedAnnotationSet, PendingAnnotation, javascript_trim, parse_invocation_context,
    selected_text_from_invocation,
};

const USAGE: &str = "Usage: herdr-annotate \
    <capture|copy-context|copy-archive|paste-archive|send-archive|editor|manage|manager>";

/// Dispatch one native binary subcommand.
pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("capture") if args.len() == 1 => capture().inspect_err(|message| {
            notify("Annotate failed", Some(message));
        }),
        Some("copy-context") if args.len() == 1 => copy_context().inspect_err(|message| {
            notify("Copy failed", Some(message));
        }),
        Some("copy-archive") if args.len() == 1 => copy_archive(),
        Some("paste-archive") if args.len() == 1 => deliver_archive(Delivery::Paste),
        Some("send-archive") if args.len() == 1 => deliver_archive(Delivery::Send),
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

/// The notification and exit status one copy-, paste- or send-and-archive action reports.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveReport {
    title: String,
    body: String,
    failure: bool,
}

/// Map one copy-and-archive outcome to the action's notification and exit status.
///
/// `loaded_empty` separates the nothing-to-do case from a real failure: both are `StayOpen`,
/// but an empty store is reported like `copy-context` and returns success.
fn copy_archive_report(outcome: CopyAndArchiveOutcome, loaded_empty: bool) -> ArchiveReport {
    match outcome {
        CopyAndArchiveOutcome::Close { archived_count } => ArchiveReport {
            title: "Annotations copied and archived".to_owned(),
            body: format!(
                "{archived_count} annotation{} copied as Markdown and archived.",
                if archived_count == 1 { "" } else { "s" }
            ),
            failure: false,
        },
        CopyAndArchiveOutcome::ArchivedActiveRetained { message } => ArchiveReport {
            title: "Copy and archive incomplete".to_owned(),
            body: format!("Copied and archived, but active annotations remain: {message}"),
            failure: true,
        },
        CopyAndArchiveOutcome::StayOpen { .. } if loaded_empty => ArchiveReport {
            title: "No annotations".to_owned(),
            body: "There is nothing to copy yet.".to_owned(),
            failure: false,
        },
        CopyAndArchiveOutcome::StayOpen { message } => ArchiveReport {
            title: "Copy and archive failed".to_owned(),
            body: message,
            failure: true,
        },
    }
}

fn copy_archive() -> Result<(), String> {
    let Some(dir) = state_dir() else {
        let message = "HERDR_PLUGIN_STATE_DIR is not set".to_owned();
        notify("Copy and archive failed", Some(&message));
        return Err(message);
    };

    let loaded_empty = Cell::new(false);
    let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
        load_active: || {
            let loaded = load_annotations(&dir);
            if matches!(&loaded, Ok(active) if active.is_empty()) {
                loaded_empty.set(true);
            }
            loaded
        },
        deliver: |text: String| write_clipboard(&text),
        save_archive: |archive: ArchivedAnnotationSet| append_archived_set(&dir, &archive),
        remove_active: |ids: Vec<String>| remove_annotations_by_id(&dir, &ids),
        create_archive_id: || Uuid::new_v4().to_string(),
        now: now_iso,
    });

    let report = copy_archive_report(outcome, loaded_empty.get());
    notify(&report.title, Some(&report.body));
    if report.failure {
        return Err(report.body);
    }
    Ok(())
}

/// Map one paste- or send-and-archive outcome to the action's notification and exit status.
///
/// `loaded_empty` is read as in `copy_archive_report`. `delivered` separates a refused delivery,
/// where nothing reached the agent, from an archive failure after the agent already has the text:
/// both are `StayOpen`, but only the second must not suggest trying again.
fn deliver_archive_report(
    delivery: Delivery,
    outcome: CopyAndArchiveOutcome,
    loaded_empty: bool,
    delivered: bool,
) -> ArchiveReport {
    let (action, landed) = match delivery {
        Delivery::Paste => ("Paste", "pasted into the agent's prompt"),
        Delivery::Send => ("Send", "sent to the agent"),
    };
    let past = delivery.past();
    match outcome {
        CopyAndArchiveOutcome::Close { archived_count } => ArchiveReport {
            title: format!("Annotations {past} and archived"),
            body: format!(
                "{archived_count} annotation{} {landed} and archived.",
                if archived_count == 1 { "" } else { "s" }
            ),
            failure: false,
        },
        CopyAndArchiveOutcome::ArchivedActiveRetained { message } => ArchiveReport {
            title: format!("{action} and archive incomplete"),
            body: format!(
                "{} and archived, but active annotations remain: {message}",
                capitalized(landed)
            ),
            failure: true,
        },
        CopyAndArchiveOutcome::StayOpen { .. } if loaded_empty => ArchiveReport {
            title: "No annotations".to_owned(),
            body: format!("There is nothing to {} yet.", delivery.verb()),
            failure: false,
        },
        CopyAndArchiveOutcome::StayOpen { message } if delivered => ArchiveReport {
            title: format!("Annotations {past}, not archived"),
            body: format!(
                "{}, but archiving failed, so your annotations are still active: {message}",
                capitalized(landed)
            ),
            failure: true,
        },
        CopyAndArchiveOutcome::StayOpen { message } => ArchiveReport {
            title: format!("{action} failed"),
            body: message,
            failure: true,
        },
    }
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}

fn deliver_archive(delivery: Delivery) -> Result<(), String> {
    let failed = match delivery {
        Delivery::Paste => "Paste failed",
        Delivery::Send => "Send failed",
    };
    let Some(dir) = state_dir() else {
        let message = "HERDR_PLUGIN_STATE_DIR is not set".to_owned();
        notify(failed, Some(&message));
        return Err(message);
    };
    let pane = parse_invocation_context(&invocation_context()).focused_pane_id;

    let loaded_empty = Cell::new(false);
    let delivered = Cell::new(false);
    let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
        load_active: || {
            let loaded = load_annotations(&dir);
            if matches!(&loaded, Ok(active) if active.is_empty()) {
                loaded_empty.set(true);
            }
            loaded
        },
        deliver: |text: String| {
            deliver_to_agent(delivery, pane.as_deref(), &text, run_herdr_output)?;
            delivered.set(true);
            Ok(())
        },
        save_archive: |archive: ArchivedAnnotationSet| append_archived_set(&dir, &archive),
        remove_active: |ids: Vec<String>| remove_annotations_by_id(&dir, &ids),
        create_archive_id: || Uuid::new_v4().to_string(),
        now: now_iso,
    });

    let report = deliver_archive_report(delivery, outcome, loaded_empty.get(), delivered.get());
    notify(&report.title, Some(&report.body));
    if report.failure {
        return Err(report.body);
    }
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
        assert_eq!(
            run(&["copy-archive".to_owned(), "extra".to_owned()]),
            Err(USAGE.to_owned())
        );
        for subcommand in ["paste-archive", "send-archive"] {
            assert_eq!(
                run(&[subcommand.to_owned(), "extra".to_owned()]),
                Err(USAGE.to_owned())
            );
            assert!(USAGE.contains(subcommand), "{USAGE}");
        }
    }

    #[test]
    fn copy_archive_maps_every_outcome_to_its_notification_and_exit_status() {
        assert_eq!(
            copy_archive_report(CopyAndArchiveOutcome::Close { archived_count: 1 }, false),
            ArchiveReport {
                title: "Annotations copied and archived".to_owned(),
                body: "1 annotation copied as Markdown and archived.".to_owned(),
                failure: false,
            }
        );
        assert_eq!(
            copy_archive_report(CopyAndArchiveOutcome::Close { archived_count: 3 }, false),
            ArchiveReport {
                title: "Annotations copied and archived".to_owned(),
                body: "3 annotations copied as Markdown and archived.".to_owned(),
                failure: false,
            }
        );
        assert_eq!(
            copy_archive_report(
                CopyAndArchiveOutcome::StayOpen {
                    message: "Nothing to copy and archive.".to_owned(),
                },
                true,
            ),
            ArchiveReport {
                title: "No annotations".to_owned(),
                body: "There is nothing to copy yet.".to_owned(),
                failure: false,
            }
        );
        assert_eq!(
            copy_archive_report(
                CopyAndArchiveOutcome::StayOpen {
                    message: "clipboard write failed".to_owned(),
                },
                false,
            ),
            ArchiveReport {
                title: "Copy and archive failed".to_owned(),
                body: "clipboard write failed".to_owned(),
                failure: true,
            }
        );
        assert_eq!(
            copy_archive_report(
                CopyAndArchiveOutcome::ArchivedActiveRetained {
                    message: "store is busy".to_owned(),
                },
                false,
            ),
            ArchiveReport {
                title: "Copy and archive incomplete".to_owned(),
                body: "Copied and archived, but active annotations remain: store is busy"
                    .to_owned(),
                failure: true,
            }
        );
    }

    #[test]
    fn paste_and_send_archive_map_every_outcome_to_their_own_wording() {
        assert_eq!(
            deliver_archive_report(
                Delivery::Paste,
                CopyAndArchiveOutcome::Close { archived_count: 1 },
                false,
                true
            ),
            ArchiveReport {
                title: "Annotations pasted and archived".to_owned(),
                body: "1 annotation pasted into the agent's prompt and archived.".to_owned(),
                failure: false,
            }
        );
        assert_eq!(
            deliver_archive_report(
                Delivery::Send,
                CopyAndArchiveOutcome::Close { archived_count: 3 },
                false,
                true
            ),
            ArchiveReport {
                title: "Annotations sent and archived".to_owned(),
                body: "3 annotations sent to the agent and archived.".to_owned(),
                failure: false,
            }
        );
        for (delivery, body) in [
            (Delivery::Paste, "There is nothing to paste yet."),
            (Delivery::Send, "There is nothing to send yet."),
        ] {
            assert_eq!(
                deliver_archive_report(
                    delivery,
                    CopyAndArchiveOutcome::StayOpen {
                        message: "Nothing to copy and archive.".to_owned(),
                    },
                    true,
                    false
                ),
                ArchiveReport {
                    title: "No annotations".to_owned(),
                    body: body.to_owned(),
                    failure: false,
                }
            );
        }
        let refusal = "The agent is waiting on a prompt. Nothing was sent; your annotations are still active.";
        assert_eq!(
            deliver_archive_report(
                Delivery::Send,
                CopyAndArchiveOutcome::StayOpen {
                    message: refusal.to_owned(),
                },
                false,
                false
            ),
            ArchiveReport {
                title: "Send failed".to_owned(),
                body: refusal.to_owned(),
                failure: true,
            }
        );
        assert_eq!(
            deliver_archive_report(
                Delivery::Send,
                CopyAndArchiveOutcome::StayOpen {
                    message: "archive store is busy".to_owned(),
                },
                false,
                true
            ),
            ArchiveReport {
                title: "Annotations sent, not archived".to_owned(),
                body: "Sent to the agent, but archiving failed, so your annotations are still \
                       active: archive store is busy"
                    .to_owned(),
                failure: true,
            }
        );
        assert_eq!(
            deliver_archive_report(
                Delivery::Paste,
                CopyAndArchiveOutcome::ArchivedActiveRetained {
                    message: "store is busy".to_owned(),
                },
                false,
                true
            ),
            ArchiveReport {
                title: "Paste and archive incomplete".to_owned(),
                body: "Pasted into the agent's prompt and archived, but active annotations \
                       remain: store is busy"
                    .to_owned(),
                failure: true,
            }
        );
    }
}
