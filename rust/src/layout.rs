//! Comment-editor layout in terminal cells.

use crate::width::char_width;

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
    let mut lines = vec![String::new()];
    let mut row = 0;
    let mut col = 0;
    let mut cursor_row = 0;
    let mut cursor_col = 0;
    for index in 0..=comment.len() {
        let cells = comment.get(index).copied().map_or(0, char_width);
        if col > 0 && col + cells > safe_width {
            lines.push(String::new());
            row += 1;
            col = 0;
        }
        if index == cursor {
            cursor_row = row;
            cursor_col = col;
        }
        let Some(character) = comment.get(index).copied() else {
            break;
        };
        if character == '\n' {
            lines.push(String::new());
            row += 1;
            col = 0;
        } else {
            if let Some(line) = lines.get_mut(row) {
                line.push(character);
            }
            col += cells;
        }
    }
    CommentLayout {
        lines,
        cursor_row,
        cursor_col,
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
}
