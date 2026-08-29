//! Persisted annotation and Herdr invocation wire types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Herdr invocation fields retained as useful annotation provenance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_agent: Option<String>,
}

/// Clipboard text and provenance waiting for a user comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAnnotation {
    pub selected_text: String,
    pub context: InvocationContext,
    pub captured_at: String,
}

/// A saved annotation with a non-empty user comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub selected_text: String,
    pub context: InvocationContext,
    pub captured_at: String,
    pub id: String,
    pub comment: String,
    pub created_at: String,
}

impl Annotation {
    /// Convert a pending record and comment to the persisted field order used by TypeScript.
    pub fn from_pending(
        pending: PendingAnnotation,
        id: String,
        comment: String,
        created_at: String,
    ) -> Self {
        Self {
            selected_text: pending.selected_text,
            context: pending.context,
            captured_at: pending.captured_at,
            id,
            comment,
            created_at,
        }
    }
}

/// One recoverable set of annotations moved out of the active list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedAnnotationSet {
    pub version: u8,
    pub id: String,
    pub archived_at: String,
    pub annotations: Vec<Annotation>,
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Match ECMAScript `String.prototype.trim` for blank-value checks.
pub(crate) fn javascript_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

/// Parse untrusted Herdr context JSON into the small provenance shape the plugin stores.
pub fn parse_invocation_context(value: &Value) -> InvocationContext {
    if !value.is_object() {
        return InvocationContext::default();
    }
    InvocationContext {
        workspace_id: optional_string(value, "workspace_id"),
        workspace_label: optional_string(value, "workspace_label"),
        tab_id: optional_string(value, "tab_id"),
        tab_label: optional_string(value, "tab_label"),
        focused_pane_id: optional_string(value, "focused_pane_id"),
        focused_pane_cwd: optional_string(value, "focused_pane_cwd"),
        focused_pane_agent: optional_string(value, "focused_pane_agent"),
    }
}

/// Read a non-empty terminal selection from Herdr's plugin invocation context.
pub fn selected_text_from_invocation(value: &Value) -> Option<String> {
    let selected = optional_string(value, "selected_text")?;
    (!javascript_trim(&selected).is_empty()).then_some(selected)
}

/// Build a pending annotation from selected text supplied by a Herdr pane invocation.
pub fn pending_annotation_from_invocation(
    value: &Value,
    captured_at: impl Into<String>,
) -> Option<PendingAnnotation> {
    Some(PendingAnnotation {
        selected_text: selected_text_from_invocation(value)?,
        context: parse_invocation_context(value),
        captured_at: captured_at.into(),
    })
}

/// Parse a pending-annotation file, returning `None` when required fields are invalid.
pub fn parse_pending_annotation(value: &Value) -> Option<PendingAnnotation> {
    if !value.is_object() {
        return None;
    }
    Some(PendingAnnotation {
        selected_text: optional_string(value, "selectedText")?,
        context: value
            .get("context")
            .map_or_else(InvocationContext::default, parse_invocation_context),
        captured_at: optional_string(value, "capturedAt")?,
    })
}

/// Parse one persisted JSONL record, returning `None` for malformed records.
pub fn parse_annotation(value: &Value) -> Option<Annotation> {
    let pending = parse_pending_annotation(value)?;
    let id = optional_string(value, "id")?;
    let comment = optional_string(value, "comment")?;
    let created_at = optional_string(value, "createdAt")?;
    if id.is_empty() || javascript_trim(&comment).is_empty() || created_at.is_empty() {
        return None;
    }
    Some(Annotation::from_pending(pending, id, comment, created_at))
}

/// Parse one persisted archive record without accepting partial annotation sets.
pub fn parse_archived_annotation_set(value: &Value) -> Option<ArchivedAnnotationSet> {
    if value.get("version").and_then(Value::as_f64) != Some(1.0) {
        return None;
    }
    let id = optional_string(value, "id")?;
    let archived_at = optional_string(value, "archivedAt")?;
    let items = value.get("annotations")?.as_array()?;
    if id.is_empty() || archived_at.is_empty() || items.is_empty() {
        return None;
    }
    let annotations = items
        .iter()
        .map(parse_annotation)
        .collect::<Option<Vec<_>>>()?;
    Some(ArchivedAnnotationSet {
        version: 1,
        id,
        archived_at,
        annotations,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use serde_json::json;

    use super::*;

    fn persisted_annotation(id: &str) -> Value {
        json!({
            "id": id,
            "selectedText": format!("selection {id}"),
            "comment": format!("comment {id}"),
            "capturedAt": "2026-08-08T00:00:00Z",
            "createdAt": "2026-08-08T00:00:01Z",
            "context": {}
        })
    }

    #[test]
    fn terminal_selection_is_returned_without_changes() {
        assert_eq!(
            selected_text_from_invocation(&json!({"selected_text": "  selected text\n"})),
            Some("  selected text\n".to_owned())
        );
    }

    #[test]
    fn missing_invalid_and_empty_selections_are_ignored() {
        for value in [
            json!({}),
            json!({"selected_text": 42}),
            json!({"selected_text": " \n\t"}),
            json!({"selected_text": "\u{feff}"}),
            Value::Null,
        ] {
            assert_eq!(selected_text_from_invocation(&value), None);
        }
    }

    #[test]
    fn editor_fallback_retains_invocation_context() {
        let pending = pending_annotation_from_invocation(
            &json!({
                "selected_text": "selected text",
                "workspace_id": "workspace-1",
                "focused_pane_cwd": "C:\\work"
            }),
            "2026-08-27T00:00:00Z",
        )
        .expect("pending");
        assert_eq!(pending.selected_text, "selected text");
        assert_eq!(pending.context.workspace_id.as_deref(), Some("workspace-1"));
        assert_eq!(
            pending.context.focused_pane_cwd.as_deref(),
            Some("C:\\work")
        );
    }

    #[test]
    fn editor_fallback_rejects_missing_or_empty_selection() {
        assert!(pending_annotation_from_invocation(&json!({}), "now").is_none());
        assert!(
            pending_annotation_from_invocation(&json!({"selected_text": " \n"}), "now").is_none()
        );
    }

    #[test]
    fn complete_versioned_archive_parses() {
        let parsed = parse_archived_annotation_set(&json!({
            "version": 1,
            "id": "archive-one",
            "archivedAt": "2026-08-26T23:32:00Z",
            "annotations": [persisted_annotation("one")]
        }))
        .expect("archive");
        assert_eq!(parsed.id, "archive-one");
        assert_eq!(
            parsed.annotations.first().map(|item| item.id.as_str()),
            Some("one")
        );
    }

    #[test]
    fn json_number_spelling_does_not_change_archive_version_semantics() {
        let parsed = parse_archived_annotation_set(&json!({
            "version": 1.0,
            "id": "archive-one",
            "archivedAt": "2026-08-26T23:32:00Z",
            "annotations": [persisted_annotation("one")]
        }));
        assert!(parsed.is_some(), "JSON.parse treats 1.0 as the number 1");
    }

    #[test]
    fn empty_partial_and_unknown_archives_are_rejected() {
        for value in [
            json!({"version": 1, "id": "empty", "archivedAt": "now", "annotations": []}),
            json!({"version": 1, "id": "partial", "archivedAt": "now", "annotations": [persisted_annotation("one"), {"id": "broken"}]}),
            json!({"version": 2, "id": "future", "archivedAt": "now", "annotations": [persisted_annotation("one")]}),
        ] {
            assert!(parse_archived_annotation_set(&value).is_none());
        }
    }

    #[test]
    fn serialization_matches_the_typescript_field_names_and_order() {
        let annotation = Annotation::from_pending(
            PendingAnnotation {
                selected_text: "selection".to_owned(),
                context: InvocationContext::default(),
                captured_at: "captured".to_owned(),
            },
            "id".to_owned(),
            "comment".to_owned(),
            "created".to_owned(),
        );
        assert_eq!(
            serde_json::to_string(&annotation).expect("json"),
            r#"{"selectedText":"selection","context":{},"capturedAt":"captured","id":"id","comment":"comment","createdAt":"created"}"#
        );
    }
}
