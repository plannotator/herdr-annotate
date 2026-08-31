//! Interactive annotation manager pane.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};
use uuid::Uuid;

use crate::archive_workflow::{
    CopyAndArchiveDependencies, CopyAndArchiveOutcome, RestoreArchiveDependencies,
    RestoreArchivedSetOutcome, copy_and_archive_annotations, restore_archived_set,
};
use crate::clipboard::write_clipboard;
use crate::editor::now_iso_for_manager;
use crate::format::{sanitize_terminal_text, wrap_text};
use crate::manager_copy::{ManagerCopyOutcome, copy_annotations};
use crate::paths::state_dir;
use crate::store::{
    append_archived_set, load_annotations, load_archived_sets, merge_annotations,
    newest_first_annotations, newest_first_archived_sets, remove_annotations_by_id,
    remove_archived_set,
};
use crate::termination::Termination;
use crate::types::{Annotation, ArchivedAnnotationSet};
use crate::width::{string_width, truncate_to_width};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagerView {
    Active,
    Archives,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Confirmation {
    None,
    ClearActive,
    DeleteArchive { archive_id: String },
}

/// Manager state, independent of the terminal backend.
#[derive(Debug)]
pub struct ManagerApp {
    dir: PathBuf,
    annotations: Vec<Annotation>,
    archives: Vec<ArchivedAnnotationSet>,
    active_selected: usize,
    archive_selected: usize,
    view: ManagerView,
    status: String,
    confirmation: Confirmation,
    quit: bool,
}

impl ManagerApp {
    /// Load both stores and start in the active-annotation view.
    pub fn load(dir: PathBuf) -> Self {
        let mut app = Self {
            dir,
            annotations: Vec::new(),
            archives: Vec::new(),
            active_selected: 0,
            archive_selected: 0,
            view: ManagerView::Active,
            status: String::new(),
            confirmation: Confirmation::None,
            quit: false,
        };
        app.reload_active();
        app.reload_archives();
        app
    }

    fn reload_active(&mut self) -> bool {
        match load_annotations(&self.dir) {
            Ok(annotations) => {
                self.annotations = newest_first_annotations(&annotations);
                self.active_selected =
                    clamp_selection(self.active_selected, self.annotations.len());
                true
            }
            Err(message) => {
                self.status = message;
                false
            }
        }
    }

    fn reload_archives(&mut self) -> bool {
        match load_archived_sets(&self.dir) {
            Ok(archives) => {
                self.archives = newest_first_archived_sets(&archives);
                self.archive_selected = clamp_selection(self.archive_selected, self.archives.len());
                true
            }
            Err(message) => {
                self.status = message;
                false
            }
        }
    }

    /// Draw the list/detail manager layout and context-specific footer.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let cols = usize::from(area.width.max(50));
        let rows = usize::from(area.height.max(14));
        let list_width = ((cols * 36) / 100).clamp(22, 36);
        let detail_left = list_width + 2;
        let detail_width = cols.saturating_sub(detail_left + 2).max(1);
        let list_rows = rows.saturating_sub(4).max(1);

        for row in 1..rows.saturating_sub(1) {
            render_line(
                frame,
                list_width,
                row,
                "│",
                1,
                Style::default().add_modifier(Modifier::DIM),
            );
        }
        match self.view {
            ManagerView::Active => {
                self.draw_active(
                    frame,
                    rows,
                    list_width,
                    detail_left,
                    detail_width,
                    list_rows,
                );
            }
            ManagerView::Archives => {
                self.draw_archives(
                    frame,
                    rows,
                    list_width,
                    detail_left,
                    detail_width,
                    list_rows,
                );
            }
        }
        render_line(
            frame,
            1,
            rows.saturating_sub(1),
            &clipped(&self.footer_text(), cols.saturating_sub(3)),
            cols.saturating_sub(2),
            Style::default().add_modifier(Modifier::DIM),
        );
    }

    fn draw_active(
        &self,
        frame: &mut Frame<'_>,
        rows: usize,
        list_width: usize,
        detail_left: usize,
        detail_width: usize,
        list_rows: usize,
    ) {
        render_line(
            frame,
            1,
            0,
            &format!("Annotations ({})  newest first", self.annotations.len()),
            list_width.saturating_sub(1),
            Style::default().add_modifier(Modifier::BOLD),
        );
        if self.annotations.is_empty() {
            render_line(
                frame,
                1,
                2,
                "No active annotations.",
                list_width.saturating_sub(1),
                Style::default().add_modifier(Modifier::DIM),
            );
            return;
        }
        let first = first_visible_index(self.active_selected, self.annotations.len(), list_rows);
        for (index, annotation) in self
            .annotations
            .iter()
            .skip(first)
            .take(list_rows)
            .enumerate()
        {
            let absolute = first + index;
            let style = if absolute == self.active_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let prefix = if absolute == self.active_selected {
                "› "
            } else {
                "  "
            };
            render_line(
                frame,
                1,
                1 + index,
                &format!(
                    "{prefix}{}",
                    clipped(&annotation.selected_text, list_width.saturating_sub(4))
                ),
                list_width.saturating_sub(1),
                style,
            );
        }
        let Some(current) = self.annotations.get(self.active_selected) else {
            return;
        };
        render_line(
            frame,
            detail_left,
            1,
            "Selected text",
            detail_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        let selected_lines = wrap_text(
            &sanitize_terminal_text(&current.selected_text),
            detail_width,
        )
        .into_iter()
        .take(7)
        .collect::<Vec<_>>();
        for (index, line) in selected_lines.iter().enumerate() {
            render_line(
                frame,
                detail_left,
                2 + index,
                line,
                detail_width,
                Style::default().add_modifier(Modifier::DIM),
            );
        }
        let comment_row = 3 + selected_lines.len().max(3);
        render_line(
            frame,
            detail_left,
            comment_row,
            "Comment",
            detail_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        for (index, line) in wrap_text(&sanitize_terminal_text(&current.comment), detail_width)
            .into_iter()
            .take(rows.saturating_sub(comment_row + 4).max(1))
            .enumerate()
        {
            render_line(
                frame,
                detail_left,
                comment_row + 1 + index,
                &line,
                detail_width,
                Style::default(),
            );
        }
        let source = [
            current.context.workspace_label.as_deref(),
            current.context.tab_label.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" / ");
        let metadata = [
            (!source.is_empty()).then_some(source),
            Some(format_timestamp(&current.created_at)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ·  ");
        if !metadata.is_empty() {
            render_line(
                frame,
                detail_left,
                rows.saturating_sub(3),
                &clipped(&metadata, detail_width),
                detail_width,
                Style::default().add_modifier(Modifier::DIM),
            );
        }
    }

    fn draw_archives(
        &self,
        frame: &mut Frame<'_>,
        rows: usize,
        list_width: usize,
        detail_left: usize,
        detail_width: usize,
        list_rows: usize,
    ) {
        render_line(
            frame,
            1,
            0,
            &format!("Archives ({})  newest first", self.archives.len()),
            list_width.saturating_sub(1),
            Style::default().add_modifier(Modifier::BOLD),
        );
        if self.archives.is_empty() {
            render_line(
                frame,
                1,
                2,
                "No archived sets.",
                list_width.saturating_sub(1),
                Style::default().add_modifier(Modifier::DIM),
            );
            return;
        }
        let first = first_visible_index(self.archive_selected, self.archives.len(), list_rows);
        for (index, archive) in self.archives.iter().skip(first).take(list_rows).enumerate() {
            let absolute = first + index;
            let style = if absolute == self.archive_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let prefix = if absolute == self.archive_selected {
                "› "
            } else {
                "  "
            };
            let label = format!(
                "{} · {}",
                format_timestamp(&archive.archived_at),
                count_label(archive.annotations.len())
            );
            render_line(
                frame,
                1,
                1 + index,
                &format!("{prefix}{}", clipped(&label, list_width.saturating_sub(4))),
                list_width.saturating_sub(1),
                style,
            );
        }
        let Some(current) = self.archives.get(self.archive_selected) else {
            return;
        };
        render_line(
            frame,
            detail_left,
            1,
            "Archived set",
            detail_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        render_line(
            frame,
            detail_left,
            2,
            &clipped(&format_timestamp(&current.archived_at), detail_width),
            detail_width,
            Style::default().add_modifier(Modifier::DIM),
        );
        render_line(
            frame,
            detail_left,
            4,
            &count_label(current.annotations.len()),
            detail_width,
            Style::default().add_modifier(Modifier::BOLD),
        );
        let visible = newest_first_annotations(&current.annotations);
        let preview_rows = rows.saturating_sub(8).max(1);
        for (index, annotation) in visible.iter().take(preview_rows).enumerate() {
            render_line(
                frame,
                detail_left,
                5 + index,
                &clipped(
                    &format!("{}. {}", index + 1, annotation.selected_text),
                    detail_width,
                ),
                detail_width,
                Style::default(),
            );
        }
        if visible.len() > preview_rows {
            render_line(
                frame,
                detail_left,
                rows.saturating_sub(3),
                &format!("… {} more", visible.len() - preview_rows),
                detail_width,
                Style::default().add_modifier(Modifier::DIM),
            );
        }
    }

    fn footer_text(&self) -> String {
        match &self.confirmation {
            Confirmation::ClearActive => {
                return "Press Shift+D again to clear all active annotations · Esc cancel"
                    .to_owned();
            }
            Confirmation::DeleteArchive { .. } => {
                return "Press d again to permanently delete this archive · Esc cancel".to_owned();
            }
            Confirmation::None => {}
        }
        if !self.status.is_empty() {
            return self.status.clone();
        }
        match self.view {
            ManagerView::Active => {
                "j/k · y copy · c all · Shift+C copy+archive · d delete · Shift+D clear · Tab archives · q".to_owned()
            }
            ManagerView::Archives => {
                "j/k · y copy · u restore · d twice delete · Tab active · q".to_owned()
            }
        }
    }

    /// Handle one keyboard event and any requested store/clipboard transition.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if key.code == KeyCode::Esc {
            if self.confirmation == Confirmation::None {
                self.quit = true;
            } else {
                self.confirmation = Confirmation::None;
                self.status.clear();
            }
            return;
        }
        if key.code == KeyCode::Char('q') {
            self.quit = true;
            return;
        }
        if key.code == KeyCode::Tab {
            self.switch_view();
            return;
        }
        self.status.clear();
        match self.view {
            ManagerView::Active => self.handle_active_key(key),
            ManagerView::Archives => self.handle_archive_key(key),
        }
    }

    fn handle_active_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('D') {
            if self.confirmation == Confirmation::ClearActive {
                self.clear_active();
            } else {
                self.confirmation = Confirmation::ClearActive;
            }
            return;
        }
        self.confirmation = Confirmation::None;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.active_selected = self.active_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.active_selected =
                    (self.active_selected + 1).min(self.annotations.len().saturating_sub(1));
            }
            KeyCode::Char('C') => self.copy_and_archive(),
            KeyCode::Char('y') => {
                let items = self
                    .annotations
                    .get(self.active_selected)
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>();
                self.copy(&items);
            }
            KeyCode::Char('c') => self.copy(&self.annotations.clone()),
            KeyCode::Char('d') => self.delete_selected_annotation(),
            KeyCode::Char('r') if self.reload_active() => {
                "Reloaded.".clone_into(&mut self.status);
            }
            _ => {}
        }
    }

    fn handle_archive_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('d') {
            let Some(current) = self.archives.get(self.archive_selected) else {
                self.confirmation = Confirmation::None;
                "No archive selected.".clone_into(&mut self.status);
                return;
            };
            let archive_id = current.id.clone();
            if self.confirmation
                == (Confirmation::DeleteArchive {
                    archive_id: archive_id.clone(),
                })
            {
                self.delete_selected_archive(&archive_id);
            } else {
                self.confirmation = Confirmation::DeleteArchive { archive_id };
            }
            return;
        }
        self.confirmation = Confirmation::None;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.archive_selected = self.archive_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.archive_selected =
                    (self.archive_selected + 1).min(self.archives.len().saturating_sub(1));
            }
            KeyCode::Char('y') => {
                let items = self
                    .archives
                    .get(self.archive_selected)
                    .map(|archive| newest_first_annotations(&archive.annotations))
                    .unwrap_or_default();
                self.copy(&items);
            }
            KeyCode::Char('u') => self.restore_selected_archive(),
            KeyCode::Char('r') if self.reload_archives() => {
                "Reloaded.".clone_into(&mut self.status);
            }
            _ => {}
        }
    }

    fn copy(&mut self, items: &[Annotation]) {
        match copy_annotations(items, write_clipboard) {
            ManagerCopyOutcome::Close => self.quit = true,
            ManagerCopyOutcome::StayOpen { message } => self.status = message,
        }
    }

    fn copy_and_archive(&mut self) {
        let dir = self.dir.clone();
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || load_annotations(&dir),
            write_clipboard: |text: String| write_clipboard(&text),
            save_archive: |archive: ArchivedAnnotationSet| append_archived_set(&dir, &archive),
            remove_active: |ids: Vec<String>| remove_annotations_by_id(&dir, &ids),
            create_archive_id: || Uuid::new_v4().to_string(),
            now: now_iso_for_manager,
        });
        match outcome {
            CopyAndArchiveOutcome::Close { .. } => self.quit = true,
            CopyAndArchiveOutcome::StayOpen { message } => self.status = message,
            CopyAndArchiveOutcome::ArchivedActiveRetained { message } => {
                self.reload_archives();
                self.status =
                    format!("Copied and archived, but active annotations remain: {message}");
            }
        }
    }

    fn delete_selected_annotation(&mut self) {
        let Some(target) = self.annotations.get(self.active_selected) else {
            return;
        };
        match remove_annotations_by_id(&self.dir, std::slice::from_ref(&target.id)) {
            Ok(()) => {
                if self.reload_active() {
                    "Annotation deleted.".clone_into(&mut self.status);
                }
            }
            Err(message) => self.status = message,
        }
    }

    fn clear_active(&mut self) {
        let ids = self
            .annotations
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        match remove_annotations_by_id(&self.dir, &ids) {
            Ok(()) => {
                self.confirmation = Confirmation::None;
                if self.reload_active() {
                    "All active annotations cleared.".clone_into(&mut self.status);
                }
            }
            Err(message) => self.status = message,
        }
    }

    fn restore_selected_archive(&mut self) {
        let Some(target) = self.archives.get(self.archive_selected).cloned() else {
            "No archive selected.".clone_into(&mut self.status);
            return;
        };
        let outcome = restore_archived_set(
            &target,
            RestoreArchiveDependencies {
                merge_active: |items: Vec<Annotation>| merge_annotations(&self.dir, &items),
                remove_archive: |id: String| remove_archived_set(&self.dir, &id),
            },
        );
        match outcome {
            RestoreArchivedSetOutcome::StayOpen { message } => self.status = message,
            RestoreArchivedSetOutcome::RestoredArchiveRetained { message, .. } => {
                if !self.reload_active() || !self.reload_archives() {
                    return;
                }
                self.status = format!("Annotations restored, but the archive remains: {message}");
            }
            RestoreArchivedSetOutcome::Restored { restored_count } => {
                if !self.reload_active() || !self.reload_archives() {
                    return;
                }
                self.status = if restored_count == 0 {
                    "Archive removed; its annotations were already active.".to_owned()
                } else {
                    format!("{} restored.", count_label(restored_count))
                };
            }
        }
    }

    fn delete_selected_archive(&mut self, archive_id: &str) {
        match remove_archived_set(&self.dir, archive_id) {
            Ok(()) => {
                self.confirmation = Confirmation::None;
                if self.reload_archives() {
                    "Archive permanently deleted.".clone_into(&mut self.status);
                }
            }
            Err(message) => self.status = message,
        }
    }

    fn switch_view(&mut self) {
        self.confirmation = Confirmation::None;
        self.status.clear();
        match self.view {
            ManagerView::Active => {
                self.view = ManagerView::Archives;
                self.reload_archives();
            }
            ManagerView::Archives => {
                self.view = ManagerView::Active;
                self.reload_active();
            }
        }
    }
}

fn clamp_selection(selected: usize, length: usize) -> usize {
    selected.min(length.saturating_sub(1))
}

fn first_visible_index(selected: usize, length: usize, visible_rows: usize) -> usize {
    selected
        .saturating_sub(visible_rows / 2)
        .min(length.saturating_sub(visible_rows))
}

fn count_label(count: usize) -> String {
    format!("{count} annotation{}", if count == 1 { "" } else { "s" })
}

fn clipped(text: &str, width: usize) -> String {
    let sanitized = sanitize_terminal_text(text);
    let value = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    if string_width(&value) <= width {
        value
    } else {
        format!("{}…", truncate_to_width(&value, width.saturating_sub(1)))
    }
}

fn format_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map_or_else(
        |_| "Invalid Date".to_owned(),
        |date| {
            date.with_timezone(&Local)
                .format("%-m/%-d/%Y, %-I:%M:%S %p")
                .to_string()
        },
    )
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

/// Run the interactive manager pane from Herdr's environment.
pub fn run() -> Result<(), String> {
    let dir = state_dir().ok_or_else(|| "HERDR_PLUGIN_STATE_DIR is not set".to_owned())?;
    let mut app = ManagerApp::load(dir);
    let termination = Termination::install();
    let mut terminal = ratatui::init();
    let result = (|| -> Result<(), String> {
        while !app.quit && !termination.requested() {
            terminal
                .draw(|frame| app.draw(frame))
                .map_err(|error| error.to_string())?;
            while !termination.requested()
                && !event::poll(Duration::from_millis(50)).map_err(|error| error.to_string())?
            {
            }
            if termination.requested() {
                break;
            }
            if let Event::Key(key) = event::read().map_err(|error| error.to_string())? {
                app.handle_key(key);
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

    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::store::{append_annotation, append_archived_set};
    use crate::types::InvocationContext;

    use super::*;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn directory() -> PathBuf {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "herdr-annotate-manager-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temporary directory");
        dir
    }

    fn annotation(id: &str) -> Annotation {
        Annotation {
            selected_text: format!("selection {id}"),
            context: InvocationContext {
                workspace_label: Some("api".to_owned()),
                tab_label: Some("server".to_owned()),
                ..InvocationContext::default()
            },
            captured_at: "2026-08-08T00:00:00Z".to_owned(),
            id: id.to_owned(),
            comment: format!("comment {id}"),
            created_at: "2026-08-08T00:00:01Z".to_owned(),
        }
    }

    fn rows(app: &ManagerApp) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(98, 28)).expect("terminal");
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
    fn active_frame_is_newest_first_and_has_detail_and_keys() {
        let dir = directory();
        append_annotation(&dir, &annotation("one")).expect("one");
        append_annotation(&dir, &annotation("two")).expect("two");
        let app = ManagerApp::load(dir.clone());
        let frame = rows(&app);
        assert!(
            frame
                .first()
                .is_some_and(|row| row.contains("Annotations (2)"))
        );
        let one = frame
            .iter()
            .position(|row| row.contains("selection one"))
            .expect("one");
        let two = frame
            .iter()
            .position(|row| row.contains("selection two"))
            .expect("two");
        assert!(two < one);
        assert!(frame.iter().any(|row| row.contains("comment two")));
        assert!(frame.iter().any(|row| row.contains("Shift+C copy+archive")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_frame_and_delete_confirmation_render_headlessly() {
        let dir = directory();
        append_archived_set(
            &dir,
            &ArchivedAnnotationSet {
                version: 1,
                id: "archive-one".to_owned(),
                archived_at: "2026-08-26T23:32:00Z".to_owned(),
                annotations: vec![annotation("one")],
            },
        )
        .expect("archive");
        let mut app = ManagerApp::load(dir.clone());
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        let frame = rows(&app);
        assert!(
            frame
                .first()
                .is_some_and(|row| row.contains("Archives (1)"))
        );
        assert!(frame.iter().any(|row| row.contains("Archived set")));
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(rows(&app).iter().any(|row| row.contains("Press d again")));
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(!app.quit);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_preview_uses_the_typescript_detail_width() {
        let dir = directory();
        let mut item = annotation("long");
        item.selected_text = "x".repeat(100);
        append_archived_set(
            &dir,
            &ArchivedAnnotationSet {
                version: 1,
                id: "archive-long".to_owned(),
                archived_at: "2026-08-26T23:32:00Z".to_owned(),
                annotations: vec![item],
            },
        )
        .expect("archive");
        let mut app = ManagerApp::load(dir.clone());
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        let frame = rows(&app);
        let preview = frame.get(5).expect("archive preview row");
        assert_eq!(preview.chars().nth(95), Some('…'));
        assert_eq!(preview.chars().nth(96), Some(' '));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clear_requires_shift_d_twice() {
        let dir = directory();
        append_annotation(&dir, &annotation("one")).expect("annotation");
        let mut app = ManagerApp::load(dir.clone());
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(load_annotations(&dir).expect("still active").len(), 1);
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert!(load_annotations(&dir).expect("cleared").is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
