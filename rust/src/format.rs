//! Terminal-safe text and Markdown export.

use crate::types::Annotation;
use crate::width::char_width;

/// Remove terminal control characters while retaining useful whitespace.
pub fn sanitize_terminal_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character == '\t' {
                "    ".chars().collect::<Vec<_>>()
            } else if (character <= '\u{0008}')
                || matches!(character, '\u{000b}' | '\u{000c}')
                || ('\u{000e}'..='\u{001f}').contains(&character)
                || character == '\u{007f}'
            {
                Vec::new()
            } else {
                vec![character]
            }
        })
        .collect()
}

/// Wrap text to terminal-cell-width lines while preserving explicit newlines.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let safe_width = width.max(1);
    let normalized = text.replace("\r\n", "\n");
    let mut output = Vec::new();
    for source_line in normalized.split('\n') {
        if source_line.is_empty() {
            output.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut used = 0;
        for character in source_line.chars() {
            let cells = char_width(character);
            if used + cells > safe_width && !line.is_empty() {
                output.push(line);
                line = String::new();
                used = 0;
            }
            line.push(character);
            used += cells;
        }
        output.push(line);
    }
    output
}

fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut current = 0;
    for character in text.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

/// Format saved annotations as the Markdown feedback Plannotator hands a coding agent.
///
/// The shape follows plannotator-tui's export: `# Annotations on …`, then one numbered
/// `## Annotation N` per note in the order given, the quoted selection, and the comment as a
/// blockquote. Where Plannotator puts the line number in the heading, a terminal selection puts
/// the Herdr workspace and tab it came from. A one-line selection is quoted inline; a selection
/// spanning lines is fenced so its layout survives and quoted Markdown cannot escape.
pub fn format_annotations(annotations: &[Annotation]) -> String {
    let sections = annotations
        .iter()
        .enumerate()
        .map(|(index, annotation)| {
            let source = [
                annotation.context.workspace_label.as_deref(),
                annotation.context.tab_label.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" / ");
            let location = if source.is_empty() {
                String::new()
            } else {
                format!(" ({source})")
            };
            let selection = annotation.selected_text.trim_matches(['\r', '\n']);
            let quoted = if selection.contains('\n') {
                let fence = fence_for(selection);
                format!("Comment on:\n{fence}\n{selection}\n{fence}")
            } else {
                format!("Comment on: \"{}\"", selection.trim())
            };
            let comment = annotation
                .comment
                .trim()
                .replace("\r\n", "\n")
                .replace('\n', "\n> ");
            format!(
                "## Annotation {}{location}\n{quoted}\n> {comment}",
                index + 1
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("# Annotations on terminal selections\n\n{sections}\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use crate::types::{Annotation, InvocationContext};

    use super::*;

    fn annotation(selection: &str) -> Annotation {
        Annotation {
            selected_text: selection.to_owned(),
            context: InvocationContext {
                workspace_label: Some("api".to_owned()),
                tab_label: Some("server".to_owned()),
                ..InvocationContext::default()
            },
            captured_at: "captured".to_owned(),
            id: "one".to_owned(),
            comment: "Check the database first.".to_owned(),
            created_at: "created".to_owned(),
        }
    }

    #[test]
    fn wrapping_preserves_newlines_and_uses_cells() {
        assert_eq!(wrap_text("abcdef\nxy", 3), ["abc", "def", "xy"]);
        assert_eq!(wrap_text("한글한글", 4), ["한글", "한글"]);
        assert_eq!(wrap_text("한글한", 5), ["한글", "한"]);
        assert_eq!(wrap_text("a한b한", 4), ["a한b", "한"]);
    }

    #[test]
    fn terminal_display_strips_control_characters() {
        assert_eq!(
            sanitize_terminal_text("safe\u{001b}[2J\ttext\nnext"),
            "safe[2J    text\nnext"
        );
    }

    #[test]
    fn markdown_matches_the_plannotator_feedback_shape() {
        let mut second = annotation("line one\nline two\n");
        second.comment = "Why twice?\nPick one.".to_owned();
        second.context = InvocationContext::default();
        let output = format_annotations(&[annotation("failed to connect"), second]);
        assert_eq!(
            output,
            "# Annotations on terminal selections\n\n\
             ## Annotation 1 (api / server)\n\
             Comment on: \"failed to connect\"\n\
             > Check the database first.\n\n\
             ## Annotation 2\n\
             Comment on:\n```\nline one\nline two\n```\n\
             > Why twice?\n> Pick one.\n"
        );
    }

    #[test]
    fn markdown_keeps_the_given_order() {
        let mut second = annotation("second");
        second.comment = "comment two".to_owned();
        let output = format_annotations(&[annotation("first"), second]);
        let first = output.find("\"first\"").expect("first");
        let second = output.find("\"second\"").expect("second");
        assert!(first < second);
    }

    #[test]
    fn markdown_uses_a_longer_fence_for_backticks() {
        let output = format_annotations(&[annotation("```example```\nmore")]);
        assert!(output.contains("````\n```example```\nmore\n````"));
    }
}
