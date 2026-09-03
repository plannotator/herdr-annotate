//! Comment-editor layout in terminal cells.

use crate::width::{graphemes, string_width};

/// Laid-out comment lines and cursor position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentLayout {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

/// Lay the comment out and report the cursor in terminal cells.
pub fn layout_comment(comment: &[char], cursor: usize, width: usize) -> CommentLayout {
    let safe_width = width.max(1);
    let text = comment.iter().collect::<String>();
    let mut lines = vec![String::new()];
    let mut row = 0;
    let mut col = 0;
    let mut cursor_row = 0;
    let mut cursor_col = 0;
    let mut scalar_index = 0;
    for grapheme in graphemes(&text) {
        let length = grapheme.chars().count();
        let end = scalar_index + length;
        let cells = string_width(grapheme);
        if col > 0 && col + cells > safe_width {
            lines.push(String::new());
            row += 1;
            col = 0;
        }
        if cursor == scalar_index {
            cursor_row = row;
            cursor_col = col;
        }
        if grapheme == "\n" {
            lines.push(String::new());
            row += 1;
            col = 0;
        } else {
            if let Some(line) = lines.get_mut(row) {
                line.push_str(grapheme);
            }
            col += cells;
        }
        if cursor > scalar_index && cursor < end {
            cursor_row = row;
            cursor_col = col;
        }
        scalar_index = end;
    }
    if cursor >= comment.len() {
        cursor_row = row;
        cursor_col = col;
    }
    CommentLayout {
        lines,
        cursor_row,
        cursor_col,
    }
}

pub fn cursor_at_visual_position(
    comment: &[char],
    row: usize,
    column: usize,
    width: usize,
) -> usize {
    let safe_width = width.max(1);
    let text = comment.iter().collect::<String>();
    let mut visual_row = 0;
    let mut visual_column = 0;
    let mut scalar_index = 0;

    for grapheme in graphemes(&text) {
        let length = grapheme.chars().count();
        let end = scalar_index + length;
        let cells = string_width(grapheme);
        if grapheme == "\n" {
            if visual_row == row {
                return scalar_index;
            }
            visual_row += 1;
            visual_column = 0;
            scalar_index = end;
            continue;
        }
        if visual_column > 0 && visual_column + cells > safe_width {
            if visual_row == row {
                return scalar_index;
            }
            visual_row += 1;
            visual_column = 0;
        }
        if visual_row == row {
            if column < visual_column {
                return scalar_index;
            }
            if cells > 0 && column < visual_column + cells {
                return if column == visual_column {
                    scalar_index
                } else {
                    end
                };
            }
        }
        visual_column += cells;
        scalar_index = end;
    }

    comment.len()
}

pub fn editor_viewport_start(
    current_start: usize,
    cursor_row: usize,
    line_count: usize,
    visible_rows: usize,
) -> usize {
    let rows = visible_rows.max(1);
    let max_start = line_count.saturating_sub(rows);
    let start = current_start.min(max_start);
    if cursor_row < start {
        cursor_row
    } else if cursor_row >= start + rows {
        cursor_row.saturating_sub(rows - 1).min(max_start)
    } else {
        start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_columns_count_terminal_cells() {
        let wide = "한글".chars().collect::<Vec<_>>();
        assert_eq!(layout_comment(&wide, wide.len(), 40).cursor_col, 4);
        let narrow = "abc".chars().collect::<Vec<_>>();
        assert_eq!(layout_comment(&narrow, narrow.len(), 40).cursor_col, 3);
        let mixed = "a한b".chars().collect::<Vec<_>>();
        assert_eq!(layout_comment(&mixed, mixed.len(), 40).cursor_col, 4);
        assert_eq!(layout_comment(&mixed, 2, 40).cursor_col, 3);
    }

    #[test]
    fn wide_characters_do_not_straddle_the_edge() {
        let comment = "한글한".chars().collect::<Vec<_>>();
        let result = layout_comment(&comment, comment.len(), 5);
        assert_eq!(result.lines, ["한글", "한"]);
        assert_eq!((result.cursor_row, result.cursor_col), (1, 2));
    }

    #[test]
    fn explicit_newlines_are_preserved() {
        let comment = "한\n글".chars().collect::<Vec<_>>();
        let result = layout_comment(&comment, comment.len(), 40);
        assert_eq!(result.lines, ["한", "글"]);
        assert_eq!((result.cursor_row, result.cursor_col), (1, 2));
    }

    #[test]
    fn emoji_graphemes_occupy_terminal_cells() {
        for text in ["🇺🇸", "👨‍👩‍👧‍👦", "1️⃣"] {
            let comment = text.chars().collect::<Vec<_>>();
            assert_eq!(layout_comment(&comment, comment.len(), 40).cursor_col, 2);
        }
    }
    #[test]
    fn cursor_mapping_uses_glyph_cell_boundaries_and_wraps() {
        let comment = "a한b".chars().collect::<Vec<_>>();
        assert_eq!(cursor_at_visual_position(&comment, 0, 0, 3), 0);
        assert_eq!(cursor_at_visual_position(&comment, 0, 1, 3), 1);
        assert_eq!(cursor_at_visual_position(&comment, 0, 2, 3), 2);
        assert_eq!(cursor_at_visual_position(&comment, 0, 3, 3), 2);
        assert_eq!(cursor_at_visual_position(&comment, 1, 0, 3), 2);
    }

    #[test]
    fn cursor_mapping_keeps_combining_marks_with_their_base_glyph() {
        let narrow = "a\u{301}b".chars().collect::<Vec<_>>();
        assert_eq!(cursor_at_visual_position(&narrow, 0, 1, 8), 2);
        let wide = "한\u{301}b".chars().collect::<Vec<_>>();
        assert_eq!(cursor_at_visual_position(&wide, 0, 1, 8), 2);
    }

    #[test]
    fn cursor_mapping_returns_only_emoji_grapheme_boundaries() {
        for text in ["🇺🇸", "👨‍👩‍👧‍👦", "1️⃣"] {
            let comment = format!("{text}x").chars().collect::<Vec<_>>();
            let boundary = text.chars().count();
            assert_eq!(cursor_at_visual_position(&comment, 0, 0, 40), 0);
            assert_eq!(cursor_at_visual_position(&comment, 0, 1, 40), boundary);
        }
    }

    #[test]
    fn viewport_start_preserves_visible_cursor_rows() {
        assert_eq!(editor_viewport_start(3, 3, 5, 2), 3);
        assert_eq!(editor_viewport_start(3, 2, 5, 2), 2);
        assert_eq!(editor_viewport_start(1, 4, 5, 2), 3);
    }

    #[test]
    fn cursor_mapping_preserves_explicit_newlines_and_blank_rows() {
        let comment = "a\n\nb".chars().collect::<Vec<_>>();
        assert_eq!(cursor_at_visual_position(&comment, 0, 4, 8), 1);
        assert_eq!(cursor_at_visual_position(&comment, 1, 0, 8), 2);
        assert_eq!(cursor_at_visual_position(&comment, 2, 0, 8), 3);
        assert_eq!(cursor_at_visual_position(&comment, 2, 1, 8), 4);
        assert_eq!(cursor_at_visual_position(&comment, 3, 0, 8), comment.len());
    }

    #[test]
    fn cursor_mapping_reaches_a_full_width_line_end() {
        let comment = "abcdefghijklmnop".chars().collect::<Vec<_>>();
        assert_eq!(
            cursor_at_visual_position(&comment, 0, 16, 16),
            comment.len()
        );
    }
}
