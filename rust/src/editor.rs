//! Interactive comment editor pane.

use std::path::Path;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};
use serde_json::Value;
use uuid::Uuid;

use crate::format::{sanitize_terminal_text, wrap_text};
use crate::layout::layout_comment;
use crate::paths::state_dir;
use crate::store::append_annotation;
use crate::types::{
    Annotation, PendingAnnotation, javascript_trim, parse_pending_annotation,
    pending_annotation_from_invocation,
};
use crate::width::{char_width, string_width, truncate_to_width};

#[cfg(test)]
const DEFAULT_COLS: u16 = 86;
#[cfg(test)]
const DEFAULT_ROWS: u16 = 22;

/// Editor state, independent of the terminal backend.
#[derive(Debug)]
pub struct EditorApp {
    pending: PendingAnnotation,
    comment: Vec<char>,
    cursor: usize,
    status: String,
    quit: bool,
}

impl EditorApp {
    /// Start an empty comment for a captured selection.
    pub fn new(pending: PendingAnnotation) -> Self {
        Self {
            pending,
            comment: Vec::new(),
            cursor: 0,
            status: String::new(),
            quit: false,
        }
    }

    /// Draw the same selected-text, comment, and footer regions as the TypeScript editor.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let cols = usize::from(area.width.max(20));
        let rows = usize::from(area.height.max(10));
        let left = 2usize;
        let inner_width = cols.saturating_sub(4).max(1);
        let selection_rows = ((rows.saturating_sub(6)) / 2).clamp(3, 7);
        let editor_rows = rows.saturating_sub(selection_rows + 5).max(1);
        let wrapped_selection = wrap_text(
            &sanitize_terminal_text(&self.pending.selected_text),
            inner_width,
        );
        let selected = wrapped_selection.iter().take(selection_rows);
        let editing = layout_comment(&self.comment, self.cursor, inner_width);
        let editor_start = editing
            .cursor_row
            .saturating_sub(editor_rows.saturating_sub(1));

        render_line(
            frame,
            left,
            1,
            "Selected text",
            inner_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        for (index, line) in selected.enumerate() {
            render_line(
                frame,
                left,
                2 + index,
                line,
                inner_width,
                Style::default().add_modifier(Modifier::DIM),
            );
        }
        if wrapped_selection.len() > selection_rows {
            render_line(
                frame,
                left + inner_width.saturating_sub(1),
                1 + selection_rows,
                "…",
                1,
                Style::default().add_modifier(Modifier::DIM),
            );
        }

        let comment_title_row = 2 + selection_rows;
        render_line(
            frame,
            left,
            comment_title_row,
            "Comment",
            inner_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        for (index, line) in editing
            .lines
            .iter()
            .skip(editor_start)
            .take(editor_rows)
            .enumerate()
        {
            render_line(
                frame,
                left,
                comment_title_row + 1 + index,
                line,
                inner_width,
                Style::default(),
            );
        }
        let footer = if self.status.is_empty() {
            "Ctrl+S save  ·  Esc cancel  ·  Enter new line"
        } else {
            &self.status
        };
        render_line(
            frame,
            left,
            rows.saturating_sub(1),
            &truncate_to_width(footer, inner_width),
            inner_width,
            Style::default().add_modifier(Modifier::DIM),
        );

        let visual_row = editing.cursor_row.saturating_sub(editor_start);
        if editing.cursor_row >= editor_start && visual_row < editor_rows {
            let x = left.saturating_add(editing.cursor_col);
            let y = comment_title_row + 1 + visual_row;
            if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y))
                && x < area.width
                && y < area.height
            {
                frame.set_cursor_position(Position::new(x, y));
            }
        }
    }

    /// Handle one keyboard event. Returns `true` when a save should be attempted.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        self.status.clear();
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return false;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        match key.code {
            KeyCode::Esc => self.quit = true,
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.comment.remove(self.cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.comment.len() {
                    self.comment.remove(self.cursor);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.comment.len()),
            KeyCode::Up => self.move_cursor_vertical(-1),
            KeyCode::Down => self.move_cursor_vertical(1),
            KeyCode::Home => {
                while self.cursor > 0 && self.comment.get(self.cursor - 1) != Some(&'\n') {
                    self.cursor -= 1;
                }
            }
            KeyCode::End => {
                while self.cursor < self.comment.len()
                    && self.comment.get(self.cursor) != Some(&'\n')
                {
                    self.cursor += 1;
                }
            }
            KeyCode::Enter => self.insert('\n'),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(character);
            }
            _ => {}
        }
        false
    }

    fn insert(&mut self, character: char) {
        self.comment.insert(self.cursor, character);
        self.cursor += 1;
    }

    fn move_cursor_vertical(&mut self, delta: isize) {
        let before = self.comment.iter().take(self.cursor).collect::<String>();
        let row = before.split('\n').count().saturating_sub(1);
        let col = before.rsplit('\n').next().map_or(0, string_width);
        let joined = self.comment.iter().collect::<String>();
        let lines = joined.split('\n').collect::<Vec<_>>();
        let target_row = row
            .saturating_add_signed(delta)
            .min(lines.len().saturating_sub(1));
        let mut next = lines
            .iter()
            .take(target_row)
            .map(|line| line.chars().count() + 1)
            .sum::<usize>();
        let mut used = 0;
        for character in lines.get(target_row).copied().unwrap_or_default().chars() {
            let width = char_width(character);
            if used + width > col {
                break;
            }
            used += width;
            next += 1;
        }
        self.cursor = next;
    }

    fn save(&mut self, dir: Option<&Path>) -> bool {
        let value = self.comment.iter().collect::<String>();
        let value = javascript_trim(&value).to_owned();
        if value.is_empty() {
            "Write a comment before saving.".clone_into(&mut self.status);
            return false;
        }
        let Some(dir) = dir else {
            "Plugin state directory is unavailable.".clone_into(&mut self.status);
            return false;
        };
        let annotation = Annotation::from_pending(
            self.pending.clone(),
            Uuid::new_v4().to_string(),
            value,
            now_iso(),
        );
        match append_annotation(dir, &annotation) {
            Ok(()) => {
                "Saved.".clone_into(&mut self.status);
                true
            }
            Err(message) => {
                self.status = message;
                false
            }
        }
    }
}

fn render_line(frame: &mut Frame<'_>, x: usize, y: usize, text: &str, width: usize, style: Style) {
    let (Ok(x), Ok(y), Ok(width)) = (u16::try_from(x), u16::try_from(y), u16::try_from(width))
    else {
        return;
    };
    let area = frame.area();
    if x >= area.width || y >= area.height {
        return;
    }
    let width = width.min(area.width.saturating_sub(x));
    frame.render_widget(
        Paragraph::new(Line::styled(text.to_owned(), style)),
        Rect::new(x, y, width, 1),
    );
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn now_iso_for_manager() -> String {
    now_iso()
}

fn invocation_context() -> Value {
    std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

fn pending_from_env() -> Result<PendingAnnotation, String> {
    let invocation = invocation_context();
    let Some(path) = std::env::var_os("HERDR_ANNOTATE_PENDING").filter(|value| !value.is_empty())
    else {
        return pending_annotation_from_invocation(&invocation, now_iso())
            .ok_or_else(|| "Missing pending annotation".to_owned());
    };
    let path = std::path::PathBuf::from(path);
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let decoded = serde_json::from_str::<Value>(&text).map_err(|error| error.to_string())?;
    let pending = parse_pending_annotation(&decoded)
        .ok_or_else(|| "Pending annotation is invalid".to_owned())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())?;
    Ok(pending)
}

/// Run the interactive editor pane from Herdr's environment.
pub fn run() -> Result<(), String> {
    let mut app = EditorApp::new(pending_from_env()?);
    let dir = state_dir();
    let mut terminal = ratatui::init();
    let result = (|| -> Result<(), String> {
        while !app.quit {
            terminal
                .draw(|frame| app.draw(frame))
                .map_err(|error| error.to_string())?;
            let event = event::read().map_err(|error| error.to_string())?;
            if let Event::Key(key) = event
                && app.handle_key(key)
                && app.save(dir.as_deref())
            {
                terminal
                    .draw(|frame| app.draw(frame))
                    .map_err(|error| error.to_string())?;
                std::thread::sleep(Duration::from_millis(250));
                app.quit = true;
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::types::InvocationContext;

    use super::*;

    fn app() -> EditorApp {
        EditorApp::new(PendingAnnotation {
            selected_text: "first selected line\nsecond line".to_owned(),
            context: InvocationContext::default(),
            captured_at: "captured".to_owned(),
        })
    }

    fn draw(app: &EditorApp) -> Vec<String> {
        let mut terminal =
            Terminal::new(TestBackend::new(DEFAULT_COLS, DEFAULT_ROWS)).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol().to_owned())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn headless_frame_contains_selection_editor_and_keys() {
        let rows = draw(&app());
        assert!(rows.iter().any(|row| row.contains("Selected text")));
        assert!(rows.iter().any(|row| row.contains("first selected line")));
        assert!(rows.iter().any(|row| row.contains("Comment")));
        assert!(rows.iter().any(|row| row.contains("Ctrl+S save")));
    }

    #[test]
    fn editing_keys_and_empty_save_match_the_typescript_editor() {
        let mut editor = app();
        assert!(!editor.handle_key(KeyEvent::from(KeyCode::Char('한'))));
        assert!(!editor.handle_key(KeyEvent::from(KeyCode::Char('a'))));
        assert_eq!(editor.comment, ['한', 'a']);
        editor.handle_key(KeyEvent::from(KeyCode::Left));
        editor.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(editor.comment, ['a']);
        let mut empty = app();
        assert!(!empty.save(None));
        assert_eq!(empty.status, "Write a comment before saving.");
    }

    #[test]
    fn escape_and_control_c_quit() {
        let mut escape = app();
        escape.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(escape.quit);
        let mut control = app();
        control.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(control.quit);
    }
}
