//! Deliver annotations into the focused agent's prompt through the Herdr CLI.
//!
//! Sending uses `herdr agent prompt`, which pastes the text as a bracketed paste and presses
//! Enter. Herdr has no paste-without-Enter command, and `herdr pane send-text` writes raw bytes
//! without honouring the pane's bracketed-paste mode, so pasting wraps the text itself. Agent
//! TUIs enable bracketed paste; a bare shell may not and would show the markers as garbage.
//!
//! Both deliveries first ask `herdr agent get` whether the agent is ready, mirroring the checks
//! `herdr agent prompt` makes before writing. Herdr before 0.8.2 lets `agent prompt` type into an
//! approval dialog, so the plugin cannot rely on Herdr refusing a blocked agent by itself.

use serde_json::Value;

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

/// How the annotations reach the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Into the prompt, left for the user to edit and submit.
    Paste,
    /// Submitted as the agent's next message.
    Send,
}

impl Delivery {
    /// The action's verb, as in "nothing to paste".
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Paste => "paste",
            Self::Send => "send",
        }
    }

    /// The verb's past participle, as in "nothing was pasted".
    pub const fn past(self) -> &'static str {
        match self {
            Self::Paste => "pasted",
            Self::Send => "sent",
        }
    }
}

/// Herdr's `{"error":{"code","message"}}` refusal, or the raw stderr when there is no envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrError {
    pub code: Option<String>,
    pub message: String,
}

/// Parse the error envelope Herdr prints on stderr when a command fails.
pub fn parse_herdr_error(stderr: &str) -> HerdrError {
    let trimmed = stderr.trim();
    let envelope = serde_json::from_str::<Value>(trimmed).ok();
    let field = |pointer: &str| {
        envelope
            .as_ref()
            .and_then(|value| value.pointer(pointer))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    HerdrError {
        code: field("/error/code"),
        message: field("/error/message").unwrap_or_else(|| trimmed.to_owned()),
    }
}

/// Say in plain words why Herdr refused, without its JSON.
fn refusal_reason(error: &HerdrError) -> String {
    let message = error.message.trim_end_matches('.');
    match error.code.as_deref() {
        Some("agent_blocked") => "The agent is waiting on a prompt.".to_owned(),
        Some("agent_not_found") => "No agent is running in the focused pane.".to_owned(),
        Some("agent_not_ready") => "The agent is not ready for input yet.".to_owned(),
        Some("pane_not_found") => "The focused pane no longer exists.".to_owned(),
        Some(code) => format!("Herdr refused ({code}): {message}."),
        None if message.is_empty() => "Herdr failed without saying why.".to_owned(),
        None => format!("Herdr failed: {message}."),
    }
}

/// Decide from one `herdr agent get` result whether the agent can take text in its prompt.
///
/// Mirrors what `herdr agent prompt` checks before it writes: the agent must not sit at an
/// approval or question dialog, where the text would answer the dialog instead of reaching the
/// prompt; Herdr must still recognise a running agent in the pane (`agent` is cleared when the
/// agent process exits, even though a named pane still answers `agent get`); and a managed launch
/// must have finished (`launch_pending` is omitted once it has).
pub fn agent_ready(agent_get: &Result<String, String>) -> Result<(), String> {
    let record = agent_get
        .as_ref()
        .map_err(|stderr| refusal_reason(&parse_herdr_error(stderr)))?;
    let parsed = serde_json::from_str::<Value>(record).ok();
    let Some(agent) = parsed
        .as_ref()
        .and_then(|value| value.pointer("/result/agent"))
        .filter(|agent| agent.get("agent_status").and_then(Value::as_str).is_some())
    else {
        return Err("Herdr did not report the agent's state.".to_owned());
    };
    if agent.get("agent_status").and_then(Value::as_str) == Some("blocked") {
        return Err("The agent is waiting on a prompt.".to_owned());
    }
    let recognised = agent
        .get("agent")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.is_empty());
    if !recognised {
        return Err("No agent is running in the focused pane.".to_owned());
    }
    if agent
        .get("launch_pending")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("The agent is not ready for input yet.".to_owned());
    }
    Ok(())
}

/// Remove ESC so the text cannot end a bracketed paste early.
///
/// Text holding the end marker would otherwise close the paste and type the rest as keystrokes,
/// possibly including a submit. `herdr agent prompt` wraps its text without this, so both
/// deliveries need it: a probe through it lost the text before the marker and never submitted.
pub fn without_escapes(text: &str) -> String {
    text.chars()
        .filter(|character| *character != '\x1b')
        .collect()
}

/// Wrap text as one bracketed paste, without the escapes that could end it early.
pub fn bracketed_paste(text: &str) -> String {
    format!("{PASTE_START}{}{PASTE_END}", without_escapes(text))
}

/// Deliver text to the agent in `pane`, calling Herdr through `herdr`.
///
/// `herdr` takes the argument list and returns stdout, or stderr on failure. A refusal is
/// returned as a notification body and guarantees nothing was typed into the pane.
pub fn deliver_to_agent(
    delivery: Delivery,
    pane: Option<&str>,
    text: &str,
    mut herdr: impl FnMut(&[String]) -> Result<String, String>,
) -> Result<(), String> {
    let refused = |reason: String| {
        format!(
            "{reason} Nothing was {}; your annotations are still active.",
            delivery.past()
        )
    };
    let Some(pane) = pane.filter(|pane| !pane.is_empty()) else {
        return Err(refused(
            "Herdr did not say which pane is focused.".to_owned(),
        ));
    };
    let herdr_refused = |stderr: String| refused(refusal_reason(&parse_herdr_error(&stderr)));
    agent_ready(&herdr(&arguments(&["agent", "get", pane]))).map_err(refused)?;
    let write = match delivery {
        Delivery::Send => arguments(&["agent", "prompt", pane, &without_escapes(text)]),
        Delivery::Paste => arguments(&["pane", "send-text", pane, &bracketed_paste(text)]),
    };
    herdr(&write).map(drop).map_err(herdr_refused)
}

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::archive_workflow::{
        CopyAndArchiveDependencies, CopyAndArchiveOutcome, copy_and_archive_annotations,
    };
    use crate::types::{Annotation, InvocationContext};

    use super::*;

    fn envelope(code: &str, message: &str) -> String {
        format!(r#"{{"error":{{"code":"{code}","message":"{message}"}},"id":"cli:agent:get"}}"#)
    }

    fn agent_record(status: &str) -> String {
        format!(
            r#"{{"id":"cli:agent:get","result":{{"agent":{{"agent":"claude","agent_status":"{status}","pane_id":"w1:p2"}},"type":"agent_info"}}}}"#
        )
    }

    fn command(args: &[String]) -> String {
        args.iter().take(2).cloned().collect::<Vec<_>>().join(" ")
    }

    fn annotation(id: &str) -> Annotation {
        Annotation {
            selected_text: format!("selection {id}"),
            context: InvocationContext::default(),
            captured_at: "2026-09-24T00:00:00Z".to_owned(),
            id: id.to_owned(),
            comment: format!("comment {id}"),
            created_at: "2026-09-24T00:00:01Z".to_owned(),
        }
    }

    #[test]
    fn a_paste_is_the_text_between_bracketed_paste_markers() {
        assert_eq!(
            bracketed_paste("line one\nline two"),
            "\x1b[200~line one\nline two\x1b[201~"
        );
        assert_eq!(bracketed_paste(""), "\x1b[200~\x1b[201~");
        assert_eq!(bracketed_paste("한글 · é"), "\x1b[200~한글 · é\x1b[201~");
    }

    #[test]
    fn an_end_marker_in_the_text_cannot_close_the_paste_early() {
        let payload = bracketed_paste("before\x1b[201~\rafter\x1b[200~");
        assert_eq!(payload, "\x1b[200~before[201~\rafter[200~\x1b[201~");
        assert_eq!(payload.matches(PASTE_END).count(), 1);
        assert!(payload.ends_with(PASTE_END));
        assert_eq!(payload.matches('\x1b').count(), 2);
    }

    #[test]
    fn herdr_error_envelopes_parse_to_code_and_message() {
        assert_eq!(
            parse_herdr_error(&envelope("agent_blocked", "agent w1:p2 is blocked")),
            HerdrError {
                code: Some("agent_blocked".to_owned()),
                message: "agent w1:p2 is blocked".to_owned(),
            }
        );
        assert_eq!(
            parse_herdr_error(&format!(
                "{}\n",
                envelope("agent_not_found", "agent target w1:p2 not found")
            )),
            HerdrError {
                code: Some("agent_not_found".to_owned()),
                message: "agent target w1:p2 not found".to_owned(),
            }
        );
    }

    #[test]
    fn unparseable_stderr_is_kept_as_the_message() {
        assert_eq!(
            parse_herdr_error("connection refused\n"),
            HerdrError {
                code: None,
                message: "connection refused".to_owned(),
            }
        );
        assert_eq!(
            parse_herdr_error(""),
            HerdrError {
                code: None,
                message: String::new(),
            }
        );
    }

    #[test]
    fn refusals_read_as_plain_words() {
        let reason = |stderr: &str| refusal_reason(&parse_herdr_error(stderr));
        assert_eq!(
            reason(&envelope("agent_blocked", "agent w1:p2 is blocked")),
            "The agent is waiting on a prompt."
        );
        assert_eq!(
            reason(&envelope("agent_not_found", "agent target w1:p2 not found")),
            "No agent is running in the focused pane."
        );
        assert_eq!(
            reason(&envelope("socket_error", "boom.")),
            "Herdr refused (socket_error): boom."
        );
        assert_eq!(
            reason("connection refused"),
            "Herdr failed: connection refused."
        );
        assert_eq!(reason(""), "Herdr failed without saying why.");
    }

    fn agent_info(agent: &str) -> String {
        format!(r#"{{"id":"cli:agent:get","result":{{"agent":{agent},"type":"agent_info"}}}}"#)
    }

    #[test]
    fn a_recognised_agent_not_at_a_dialog_is_ready() {
        for status in ["idle", "working", "done", "unknown"] {
            assert_eq!(agent_ready(&Ok(agent_record(status))), Ok(()), "{status}");
        }
        assert_eq!(
            agent_ready(&Ok(agent_info(
                r#"{"agent":"pi","agent_status":"idle","launch_pending":false,"pane_id":"w1:p2"}"#
            ))),
            Ok(())
        );
    }

    #[test]
    fn a_blocked_agent_is_not_ready() {
        assert_eq!(
            agent_ready(&Ok(agent_record("blocked"))),
            Err("The agent is waiting on a prompt.".to_owned())
        );
    }

    #[test]
    fn an_unknown_pane_without_an_agent_is_not_ready() {
        assert_eq!(
            agent_ready(&Ok(agent_info(
                r#"{"agent_status":"unknown","pane_id":"w1:p2"}"#
            ))),
            Err("No agent is running in the focused pane.".to_owned())
        );
        assert_eq!(
            agent_ready(&Ok(agent_info(
                r#"{"agent":"","agent_status":"unknown","pane_id":"w1:p2"}"#
            ))),
            Err("No agent is running in the focused pane.".to_owned())
        );
    }

    #[test]
    fn an_exited_named_agent_is_not_ready() {
        // A named agent pane still answers `agent get` after its process exits, without `agent`.
        assert_eq!(
            agent_ready(&Ok(agent_info(
                r#"{"name":"reviewer","agent_status":"idle","pane_id":"w1:p2"}"#
            ))),
            Err("No agent is running in the focused pane.".to_owned())
        );
    }

    #[test]
    fn a_pending_launch_is_not_ready() {
        assert_eq!(
            agent_ready(&Ok(agent_info(
                r#"{"agent":"claude","agent_status":"unknown","launch_pending":true,"pane_id":"w1:p2"}"#
            ))),
            Err("The agent is not ready for input yet.".to_owned())
        );
    }

    #[test]
    fn a_refused_or_unreadable_agent_get_is_not_ready() {
        assert_eq!(
            agent_ready(&Err(envelope(
                "agent_not_found",
                "agent target w1:p2 not found"
            ))),
            Err("No agent is running in the focused pane.".to_owned())
        );
        assert_eq!(
            agent_ready(&Ok("not json".to_owned())),
            Err("Herdr did not report the agent's state.".to_owned())
        );
        assert_eq!(
            agent_ready(&Ok(r#"{"result":{"agent":{}}}"#.to_owned())),
            Err("Herdr did not report the agent's state.".to_owned())
        );
    }

    #[test]
    fn send_prompts_the_focused_agent_with_the_text_minus_escapes() {
        let calls = RefCell::new(Vec::new());
        let result = deliver_to_agent(
            Delivery::Send,
            Some("w1:p2"),
            "hi\x1b[201~\rthere",
            |args| {
                calls.borrow_mut().push(args.to_vec());
                Ok(agent_record("idle"))
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            *calls.borrow(),
            [
                vec!["agent", "get", "w1:p2"],
                vec!["agent", "prompt", "w1:p2", "hi[201~\rthere"],
            ]
        );
    }

    #[test]
    fn a_blocked_send_is_refused_before_prompting() {
        // Herdr before 0.8.2 would type a prompt into the dialog, so the check must come first.
        let calls = RefCell::new(Vec::new());
        let result = deliver_to_agent(Delivery::Send, Some("w1:p2"), "hi", |args| {
            calls.borrow_mut().push(command(args));
            Ok(agent_record("blocked"))
        });
        assert_eq!(
            result,
            Err("The agent is waiting on a prompt. Nothing was sent; your annotations are still active.".to_owned())
        );
        assert_eq!(*calls.borrow(), ["agent get"]);
    }

    #[test]
    fn herdrs_own_prompt_refusal_reports_why_and_that_nothing_was_sent() {
        let result = deliver_to_agent(Delivery::Send, Some("w1:p2"), "hi", |args| {
            if command(args) == "agent get" {
                Ok(agent_record("idle"))
            } else {
                Err(envelope("agent_blocked", "agent w1:p2 is blocked"))
            }
        });
        assert_eq!(
            result,
            Err("The agent is waiting on a prompt. Nothing was sent; your annotations are still active.".to_owned())
        );
    }

    #[test]
    fn a_missing_pane_is_refused_without_calling_herdr() {
        for pane in [None, Some("")] {
            let result = deliver_to_agent(Delivery::Paste, pane, "hi", |_| {
                Err("herdr must not be called".to_owned())
            });
            assert_eq!(
                result,
                Err("Herdr did not say which pane is focused. Nothing was pasted; your annotations are still active.".to_owned())
            );
        }
    }

    #[test]
    fn paste_checks_the_agent_before_typing_the_bracketed_text() {
        let calls = RefCell::new(Vec::new());
        let result = deliver_to_agent(Delivery::Paste, Some("w1:p2"), "hi", |args| {
            calls.borrow_mut().push(args.to_vec());
            Ok(agent_record("idle"))
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            *calls.borrow(),
            [
                vec!["agent", "get", "w1:p2"],
                vec!["pane", "send-text", "w1:p2", "\x1b[200~hi\x1b[201~"],
            ]
        );
    }

    #[test]
    fn a_refused_paste_archives_nothing() {
        let events = RefCell::new(Vec::new());
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || {
                events.borrow_mut().push("load".to_owned());
                Ok(vec![annotation("one")])
            },
            deliver: |text: String| {
                deliver_to_agent(Delivery::Paste, Some("w1:p2"), &text, |args| {
                    events.borrow_mut().push(command(args));
                    Ok(agent_record("blocked"))
                })
            },
            save_archive: |_| {
                events.borrow_mut().push("archive".to_owned());
                Ok(())
            },
            remove_active: |_| {
                events.borrow_mut().push("remove".to_owned());
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(
            outcome,
            CopyAndArchiveOutcome::StayOpen {
                message: "The agent is waiting on a prompt. Nothing was pasted; your annotations are still active.".to_owned()
            }
        );
        assert_eq!(*events.borrow(), ["load", "agent get"]);
    }

    #[test]
    fn a_send_to_an_agent_that_is_not_ready_archives_nothing() {
        let events = RefCell::new(Vec::new());
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || {
                events.borrow_mut().push("load".to_owned());
                Ok(vec![annotation("one")])
            },
            deliver: |text: String| {
                deliver_to_agent(Delivery::Send, Some("w1:p2"), &text, |args| {
                    events.borrow_mut().push(command(args));
                    Ok(agent_info(
                        r#"{"name":"reviewer","agent_status":"idle","pane_id":"w1:p2"}"#,
                    ))
                })
            },
            save_archive: |_| {
                events.borrow_mut().push("archive".to_owned());
                Ok(())
            },
            remove_active: |_| {
                events.borrow_mut().push("remove".to_owned());
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(
            outcome,
            CopyAndArchiveOutcome::StayOpen {
                message: "No agent is running in the focused pane. Nothing was sent; your annotations are still active.".to_owned()
            }
        );
        assert_eq!(*events.borrow(), ["load", "agent get"]);
    }

    #[test]
    fn a_delivered_send_is_archived_afterwards() {
        let events = RefCell::new(Vec::new());
        let prompted = RefCell::new(String::new());
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || {
                events.borrow_mut().push("load".to_owned());
                Ok(vec![annotation("one"), annotation("two")])
            },
            deliver: |text: String| {
                deliver_to_agent(Delivery::Send, Some("w1:p2"), &text, |args| {
                    events.borrow_mut().push(command(args));
                    if command(args) == "agent get" {
                        return Ok(agent_record("idle"));
                    }
                    args.get(3)
                        .cloned()
                        .unwrap_or_default()
                        .clone_into(&mut prompted.borrow_mut());
                    Ok(String::new())
                })
            },
            save_archive: |_| {
                events.borrow_mut().push("archive".to_owned());
                Ok(())
            },
            remove_active: |_| {
                events.borrow_mut().push("remove".to_owned());
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(outcome, CopyAndArchiveOutcome::Close { archived_count: 2 });
        assert_eq!(
            *events.borrow(),
            ["load", "agent get", "agent prompt", "archive", "remove"]
        );
        assert!(prompted.borrow().find("selection two") < prompted.borrow().find("selection one"));
    }
}
