//! Terminal cell-width helpers matching the TypeScript implementation.

use unicode_segmentation::{Graphemes, UnicodeSegmentation};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn graphemes(text: &str) -> Graphemes<'_> {
    text.graphemes(true)
}

/// Cells occupied by a single character. Control characters count as zero.
pub fn char_width(character: char) -> usize {
    character.width().unwrap_or(0)
}

/// Cells occupied by a string.
pub fn string_width(text: &str) -> usize {
    text.width()
}

/// Longest prefix of `text` that fits without splitting a character.
pub fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    graphemes(text)
        .take_while(|grapheme| {
            let next = string_width(grapheme);
            let fits = used + next <= width;
            if fits {
                used += next;
            }
            fits
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_widths_match_the_typescript_ranges() {
        for character in ['한', 'ㄱ', '漢', 'あ', 'Ａ'] {
            assert_eq!(char_width(character), 2);
        }
        for character in ['a', '·'] {
            assert_eq!(char_width(character), 1);
        }
        assert_eq!(char_width('\u{0007}'), 0);
    }

    #[test]
    fn string_width_adds_character_cells() {
        assert_eq!(string_width("한글abc"), 7);
        assert_eq!(string_width(""), 0);
        for text in ["🇺🇸", "👨‍👩‍👧‍👦", "1️⃣"] {
            assert_eq!(string_width(text), 2);
        }
    }

    #[test]
    fn truncation_never_splits_wide_characters() {
        assert_eq!(truncate_to_width("한글", 3), "한");
        assert_eq!(truncate_to_width("한글", 4), "한글");
        assert_eq!(truncate_to_width("한", 0), "");
        assert_eq!(truncate_to_width("🇺🇸x", 2), "🇺🇸");
    }
}
