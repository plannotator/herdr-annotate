//! Focus the Herdr pane an annotation was captured from.
//!
//! Herdr's CLI focuses a pane by id only through `herdr agent focus`, which accepts agent panes.
//! For any other pane the plugin focuses the pane's tab instead, which puts the pane on screen.
//! Herdr cannot address a position in a pane's scrollback, so the jump stops at the pane.

use serde_json::Value;

use crate::agent_delivery::parse_herdr_error;
use crate::types::InvocationContext;

/// What a jump did in Herdr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpOutcome {
    /// The source pane, or the tab that holds it, is now focused.
    Focused,
    /// Nothing was focused; the message says why.
    Unavailable(String),
}

/// Focus the pane recorded in `context`, calling Herdr through `herdr`.
///
/// `herdr` takes the argument list and returns stdout, or stderr on failure. An annotation
/// without a pane id makes no Herdr call at all.
pub fn jump_to_source(
    context: &InvocationContext,
    mut herdr: impl FnMut(&[String]) -> Result<String, String>,
) -> JumpOutcome {
    let Some(pane) = context
        .focused_pane_id
        .as_deref()
        .filter(|id| !id.trim().is_empty() && !id.starts_with('-'))
    else {
        return JumpOutcome::Unavailable(
            "This annotation has no source pane to jump to.".to_owned(),
        );
    };
    if herdr(&args(&["agent", "focus", pane])).is_ok() {
        return JumpOutcome::Focused;
    }
    let record = match herdr(&args(&["pane", "get", pane])) {
        Ok(stdout) => stdout,
        Err(stderr) => return JumpOutcome::Unavailable(failure(&stderr)),
    };
    let tab = serde_json::from_str::<Value>(&record)
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/pane/tab_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|id| !id.is_empty() && !id.starts_with('-'));
    let Some(tab) = tab else {
        return JumpOutcome::Unavailable("Herdr did not report the source pane's tab.".to_owned());
    };
    match herdr(&args(&["tab", "focus", &tab])) {
        Ok(_) => JumpOutcome::Focused,
        Err(stderr) => JumpOutcome::Unavailable(failure(&stderr)),
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Say in plain words why Herdr could not focus the source, without its JSON.
fn failure(stderr: &str) -> String {
    let error = parse_herdr_error(stderr);
    let message = error.message.trim_end_matches('.');
    match error.code.as_deref() {
        Some("pane_not_found" | "tab_not_found") => "The source pane no longer exists.".to_owned(),
        Some(code) => format!("Herdr refused ({code}): {message}."),
        None if message.is_empty() => "Herdr failed without saying why.".to_owned(),
        None => format!("Herdr failed: {message}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(pane: Option<&str>) -> InvocationContext {
        InvocationContext {
            focused_pane_id: pane.map(str::to_owned),
            ..InvocationContext::default()
        }
    }

    fn not_found(code: &str, target: &str) -> String {
        format!(r#"{{"error":{{"code":"{code}","message":"{target} not found"}},"id":"cli"}}"#)
    }

    #[test]
    fn annotation_without_a_pane_id_makes_no_herdr_call() {
        for pane in [None, Some(""), Some("  "), Some("--help")] {
            let mut calls = Vec::new();
            let outcome = jump_to_source(&context(pane), |args: &[String]| {
                calls.push(args.to_vec());
                Ok(String::new())
            });
            assert_eq!(
                outcome,
                JumpOutcome::Unavailable(
                    "This annotation has no source pane to jump to.".to_owned()
                )
            );
            assert_eq!(calls, Vec::<Vec<String>>::new());
        }
    }

    #[test]
    fn agent_pane_is_focused_directly() {
        let mut calls = Vec::new();
        let outcome = jump_to_source(&context(Some("w1:p2")), |args: &[String]| {
            calls.push(args.join(" "));
            Ok("{}".to_owned())
        });
        assert_eq!(outcome, JumpOutcome::Focused);
        assert_eq!(calls, ["agent focus w1:p2"]);
    }

    #[test]
    fn missing_pane_reports_it_and_focuses_nothing() {
        let mut calls = Vec::new();
        let outcome = jump_to_source(&context(Some("w1:p9")), |args: &[String]| {
            calls.push(args.join(" "));
            match args.first().map(String::as_str) {
                Some("agent") => Err(not_found("agent_not_found", "agent target w1:p9")),
                _ => Err(not_found("pane_not_found", "pane w1:p9")),
            }
        });
        assert_eq!(
            outcome,
            JumpOutcome::Unavailable("The source pane no longer exists.".to_owned())
        );
        assert_eq!(calls, ["agent focus w1:p9", "pane get w1:p9"]);
    }

    #[test]
    fn other_pane_focuses_its_current_tab() {
        let mut calls = Vec::new();
        let outcome = jump_to_source(&context(Some("w1:p3")), |args: &[String]| {
            calls.push(args.join(" "));
            match args.first().map(String::as_str) {
                Some("agent") => Err(not_found("agent_not_found", "agent target w1:p3")),
                Some("pane") => Ok(
                    r#"{"id":"cli:pane:get","result":{"pane":{"pane_id":"w1:p3","tab_id":"w1:t2","workspace_id":"w1"},"type":"pane_info"}}"#
                        .to_owned(),
                ),
                _ => Ok("{}".to_owned()),
            }
        });
        assert_eq!(outcome, JumpOutcome::Focused);
        assert_eq!(
            calls,
            ["agent focus w1:p3", "pane get w1:p3", "tab focus w1:t2"]
        );
    }

    #[test]
    fn herdr_failures_are_reported_without_json() {
        let outcome = jump_to_source(&context(Some("w1:p2")), |_: &[String]| {
            Err(
                r#"{"id":"cli:pane:get","error":{"code":"server_not_running","message":"no herdr server is running."}}"#
                    .to_owned(),
            )
        });
        assert_eq!(
            outcome,
            JumpOutcome::Unavailable(
                "Herdr refused (server_not_running): no herdr server is running.".to_owned()
            )
        );
        let outcome = jump_to_source(&context(Some("w1:p2")), |args: &[String]| {
            match args.first().map(String::as_str) {
                Some("pane") => Ok("not json".to_owned()),
                _ => Err(String::new()),
            }
        });
        assert_eq!(
            outcome,
            JumpOutcome::Unavailable("Herdr did not report the source pane's tab.".to_owned())
        );
    }
}
