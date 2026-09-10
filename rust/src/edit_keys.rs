//! Word and line editing keys for the comment editor.
//!
//! This mirrors `src/edit-keys.ts` character for character so both runtimes move, and kill, by the
//! same boundaries. The helpers work on the editor's `Vec<char>` buffer and its scalar cursor.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// An editing action that moves or kills more than one character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    /// Move to the start of the word before the cursor.
    WordLeft,
    /// Move to the end of the word after the cursor.
    WordRight,
    /// Move to the first character of the current line.
    LineStart,
    /// Move to the last character of the current line.
    LineEnd,
    /// Kill back to the start of the word before the cursor.
    DeleteWord,
    /// Kill back to the start of the current line.
    DeleteLine,
}

/// The characters JavaScript's `\s` class matches.
///
/// `char::is_whitespace` follows Unicode `White_Space`, which includes U+0085 and excludes U+FEFF;
/// `/\s/` does the opposite. The editor splits words with the TypeScript set so a buffer holding
/// either scalar is divided identically by both runtimes.
fn is_javascript_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{9}'..='\u{d}'
            | '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

fn is_word_char(character: Option<&char>) -> bool {
    character.is_some_and(|&character| character != '\n' && !is_javascript_whitespace(character))
}

fn is_newline(character: Option<&char>) -> bool {
    character == Some(&'\n')
}

/// Index of the start of the word before `cursor` (readline backward-word).
pub fn word_start(chars: &[char], cursor: usize) -> usize {
    let mut index = cursor.min(chars.len());
    while index > 0 && !is_word_char(chars.get(index - 1)) && !is_newline(chars.get(index - 1)) {
        index -= 1;
    }
    if index > 0 && is_newline(chars.get(index - 1)) && index == cursor {
        return index - 1;
    }
    while index > 0 && is_word_char(chars.get(index - 1)) {
        index -= 1;
    }
    index
}

/// Index of the end of the word after `cursor` (readline forward-word).
pub fn word_end(chars: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index < chars.len() && !is_word_char(chars.get(index)) && !is_newline(chars.get(index)) {
        index += 1;
    }
    if index < chars.len() && is_newline(chars.get(index)) && index == cursor {
        return index + 1;
    }
    while index < chars.len() && is_word_char(chars.get(index)) {
        index += 1;
    }
    index
}

/// Index of the first character on the line holding `cursor`.
pub fn line_start(chars: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index > 0 && !is_newline(chars.get(index - 1)) {
        index -= 1;
    }
    index
}

/// Index just past the last character on the line holding `cursor`.
pub fn line_end(chars: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index < chars.len() && !is_newline(chars.get(index)) {
        index += 1;
    }
    index
}

/// Map one crossterm key event to an editing action beyond single-character moves.
///
/// Terminals encode Option/Alt as xterm modifier 3 and Command/Super as 9. Crossterm's
/// `parse_modifiers` subtracts one and reads the result as a bit set, so modifier 3 arrives as
/// `ALT` and modifier 9 as `SUPER`; `META` is bit 32, which is modifier 33 and never Command.
/// A bare `ESC b` is parsed as the following event with `ALT` folded in, which is `Char('b')` plus
/// `ALT`, and `ESC DEL` is `Backspace` plus `ALT`. Ghostty (and most macOS terminals) rewrite
/// Cmd+Backspace to Ctrl+U and Opt+Backspace to Ctrl+W, matching the readline kill bindings.
pub fn resolve_edit_key(key: &KeyEvent) -> Option<EditAction> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let super_modifier = key.modifiers.contains(KeyModifiers::SUPER);

    match key.code {
        KeyCode::Char('u') if control => Some(EditAction::DeleteLine),
        KeyCode::Char('w') if control => Some(EditAction::DeleteWord),
        KeyCode::Backspace if alt => Some(EditAction::DeleteWord),
        KeyCode::Char('a') if control => Some(EditAction::LineStart),
        KeyCode::Char('e') if control => Some(EditAction::LineEnd),
        KeyCode::Char('b') if alt => Some(EditAction::WordLeft),
        KeyCode::Char('f') if alt => Some(EditAction::WordRight),
        KeyCode::Left if super_modifier => Some(EditAction::LineStart),
        KeyCode::Right if super_modifier => Some(EditAction::LineEnd),
        KeyCode::Left if alt || control => Some(EditAction::WordLeft),
        KeyCode::Right if alt || control => Some(EditAction::WordRight),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn word_start_skips_trailing_spaces_then_the_word() {
        let c = chars("foo bar  ");
        assert_eq!(word_start(&c, c.len()), 4);
        assert_eq!(word_start(&c, 4), 0);
        assert_eq!(word_start(&c, 0), 0);
    }

    #[test]
    fn word_start_stops_at_a_newline_before_crossing_it() {
        let c = chars("foo\nbar");
        assert_eq!(word_start(&c, 4), 3);
        assert_eq!(word_start(&c, 3), 0);
    }

    #[test]
    fn word_end_skips_leading_spaces_then_the_word() {
        let c = chars("  foo bar");
        assert_eq!(word_end(&c, 0), 5);
        assert_eq!(word_end(&c, 5), 9);
        assert_eq!(word_end(&c, 9), 9);
    }

    #[test]
    fn word_end_stops_after_a_newline() {
        let c = chars("foo\nbar");
        assert_eq!(word_end(&c, 3), 4);
    }

    #[test]
    fn cjk_runs_count_as_one_word() {
        let c = chars("한글 테스트");
        assert_eq!(word_start(&c, c.len()), 3);
        assert_eq!(word_end(&c, 0), 2);
    }

    #[test]
    fn line_start_and_line_end_stay_within_the_current_line() {
        let c = chars("ab\ncd\nef");
        assert_eq!(line_start(&c, 4), 3);
        assert_eq!(line_end(&c, 4), 5);
        assert_eq!(line_start(&c, 0), 0);
        assert_eq!(line_end(&c, 8), 8);
    }

    #[test]
    fn boundaries_handle_a_cursor_past_the_end_like_typescript() {
        // `wordStart` clamps with `Math.min`; the other three walk from the given index, so an
        // out-of-range cursor is returned unchanged by the forward helpers.
        let c = chars("ab");
        assert_eq!(word_start(&c, 9), 0);
        assert_eq!(word_end(&c, 9), 9);
        assert_eq!(line_start(&c, 9), 0);
        assert_eq!(line_end(&c, 9), 9);
    }

    #[test]
    fn javascript_whitespace_matches_the_typescript_class() {
        // U+00A0 and U+3000 are whitespace to both languages, U+FEFF only to JavaScript's `\s`,
        // and U+0085 only to `char::is_whitespace`.
        // With `char::is_whitespace` these would be 8 and 7 instead of 6 and 9.
        let c = chars("a\u{a0}b\u{3000}c\u{feff}d\u{85}e");
        assert_eq!(word_start(&c, c.len()), 6);
        assert_eq!(word_end(&c, 0), 1);
        assert_eq!(word_end(&c, 6), 9);
    }

    #[test]
    fn readline_kill_bindings_resolve() {
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(EditAction::DeleteLine)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            Some(EditAction::DeleteWord)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Backspace, KeyModifiers::ALT)),
            Some(EditAction::DeleteWord)
        );
    }

    #[test]
    fn control_a_and_control_e_move_to_line_edges() {
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            Some(EditAction::LineStart)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Some(EditAction::LineEnd)
        );
    }

    #[test]
    fn option_arrows_and_alt_letters_move_by_word() {
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Left, KeyModifiers::ALT)),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Right, KeyModifiers::ALT)),
            Some(EditAction::WordRight)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('b'), KeyModifiers::ALT)),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('f'), KeyModifiers::ALT)),
            Some(EditAction::WordRight)
        );
    }

    #[test]
    fn control_arrows_move_by_word() {
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Right, KeyModifiers::CONTROL)),
            Some(EditAction::WordRight)
        );
    }

    #[test]
    fn command_arrows_move_to_line_edges() {
        // xterm modifier 9 reaches crossterm as SUPER, and wins over a folded-in ALT.
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Left, KeyModifiers::SUPER)),
            Some(EditAction::LineStart)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Right, KeyModifiers::SUPER)),
            Some(EditAction::LineEnd)
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Left, KeyModifiers::SUPER | KeyModifiers::ALT)),
            Some(EditAction::LineStart)
        );
    }

    #[test]
    fn plain_keys_are_left_to_the_editor() {
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Left, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Backspace, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('a'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Esc, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_edit_key(&key(KeyCode::Char('u'), KeyModifiers::ALT)),
            None
        );
    }
}
