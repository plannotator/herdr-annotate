//! Terminal-safe text and Markdown export.

use crate::types::Annotation;
use crate::width::{graphemes, string_width};

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
        for grapheme in graphemes(source_line) {
            let cells = string_width(grapheme);
            if used + cells > safe_width && !line.is_empty() {
                output.push(line);
                line = String::new();
                used = 0;
            }
            line.push_str(grapheme);
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

/// Format saved annotations as portable, agent-neutral Markdown context.
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
            let fence = fence_for(&annotation.selected_text);
            let metadata = if source.is_empty() {
                String::new()
            } else {
                format!("\nSource: {source}\n")
            };
            let lines = vec![
                format!("## Annotation {}", index + 1),
                metadata,
                "Selected text:".to_owned(),
                String::new(),
                fence.clone(),
                annotation.selected_text.clone(),
                fence,
                String::new(),
                "Comment:".to_owned(),
                String::new(),
                annotation.comment.clone(),
            ];
            let mut filtered = Vec::new();
            for line in lines {
                if line.is_empty() && filtered.last().is_some_and(String::is_empty) {
                    continue;
                }
                filtered.push(line);
            }
            filtered.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("# Annotated context\n\n{sections}\n")
}

#[cfg(test)]
mod tests {
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
        assert_eq!(wrap_text("🇺🇸x", 2), ["🇺🇸", "x"]);
        assert_eq!(wrap_text("a\u{1ab0}b", 1), ["a\u{1ab0}", "b"]);
    }

    #[test]
    fn terminal_display_strips_control_characters() {
        assert_eq!(
            sanitize_terminal_text("safe\u{001b}[2J\ttext\nnext"),
            "safe[2J    text\nnext"
        );
    }

    #[test]
    fn markdown_contains_source_selection_and_comment() {
        let output = format_annotations(&[annotation("failed to connect")]);
        assert!(output.contains("# Annotated context"));
        assert!(output.contains("Source: api / server"));
        assert!(output.contains("failed to connect"));
        assert!(output.contains("Check the database first."));
    }

    #[test]
    fn markdown_uses_a_longer_fence_for_backticks() {
        let output = format_annotations(&[annotation("```example```")]);
        assert!(output.contains("````\n```example```\n````"));
    }
}
