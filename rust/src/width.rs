//! Terminal cell-width helpers matching the TypeScript implementation.

const WIDE_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x115f),
    (0x2e80, 0x303e),
    (0x3041, 0x33ff),
    (0x3400, 0x4dbf),
    (0x4e00, 0x9fff),
    (0xa000, 0xa4cf),
    (0xa960, 0xa97f),
    (0xac00, 0xd7a3),
    (0xf900, 0xfaff),
    (0xfe10, 0xfe19),
    (0xfe30, 0xfe6f),
    (0xff00, 0xff60),
    (0xffe0, 0xffe6),
    (0x1f300, 0x1f64f),
    (0x1f900, 0x1f9ff),
    (0x20000, 0x3fffd),
];

fn is_wide(code_point: u32) -> bool {
    for &(start, end) in WIDE_RANGES {
        if code_point < start {
            return false;
        }
        if code_point <= end {
            return true;
        }
    }
    false
}

/// Cells occupied by a single character. Control characters count as zero.
pub fn char_width(character: char) -> usize {
    let code_point = u32::from(character);
    if code_point < 0x20 || (0x7f..0xa0).contains(&code_point) {
        return 0;
    }
    if (0x0300..=0x036f).contains(&code_point) || (0x200b..=0x200f).contains(&code_point) {
        return 0;
    }
    if is_wide(code_point) { 2 } else { 1 }
}

/// Cells occupied by a string.
pub fn string_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// Longest prefix of `text` that fits without splitting a character.
pub fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    text.chars()
        .take_while(|character| {
            let next = char_width(*character);
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
    }

    #[test]
    fn truncation_never_splits_wide_characters() {
        assert_eq!(truncate_to_width("한글", 3), "한");
        assert_eq!(truncate_to_width("한글", 4), "한글");
        assert_eq!(truncate_to_width("한", 0), "");
    }
}
