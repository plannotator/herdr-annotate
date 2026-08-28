/**
 * Terminal cell widths.
 *
 * East Asian wide and fullwidth characters occupy two terminal cells. Counting
 * them as one shifts every column the editor computes, which misplaces the
 * cursor and lets IME preedit overlays land inside existing text.
 */

const WIDE_RANGES: readonly (readonly [number, number])[] = [
  [0x1100, 0x115f], // Hangul Jamo initial consonants
  [0x2e80, 0x303e], // CJK radicals, Kangxi, CJK symbols and punctuation
  [0x3041, 0x33ff], // Hiragana through CJK compatibility
  [0x3400, 0x4dbf], // CJK unified ideographs extension A
  [0x4e00, 0x9fff], // CJK unified ideographs
  [0xa000, 0xa4cf], // Yi syllables
  [0xa960, 0xa97f], // Hangul Jamo extended-A
  [0xac00, 0xd7a3], // Hangul syllables
  [0xf900, 0xfaff], // CJK compatibility ideographs
  [0xfe10, 0xfe19], // Vertical forms
  [0xfe30, 0xfe6f], // CJK compatibility forms, small form variants
  [0xff00, 0xff60], // Fullwidth forms
  [0xffe0, 0xffe6], // Fullwidth signs
  [0x1f300, 0x1f64f], // Emoji
  [0x1f900, 0x1f9ff], // Supplemental symbols and pictographs
  [0x20000, 0x3fffd], // CJK extensions B and beyond
];

function isWide(codePoint: number): boolean {
  for (const [start, end] of WIDE_RANGES) {
    if (codePoint < start) return false;
    if (codePoint <= end) return true;
  }
  return false;
}

/** Cells occupied by a single character. Control characters count as zero. */
export function charWidth(char: string): number {
  const codePoint = char.codePointAt(0);
  if (codePoint === undefined) return 0;
  if (codePoint < 0x20 || (codePoint >= 0x7f && codePoint < 0xa0)) return 0;
  // Combining marks attach to the preceding character.
  if (codePoint >= 0x0300 && codePoint <= 0x036f) return 0;
  if (codePoint >= 0x200b && codePoint <= 0x200f) return 0;
  return isWide(codePoint) ? 2 : 1;
}

/** Cells occupied by a string. */
export function stringWidth(text: string): number {
  let total = 0;
  for (const char of text) total += charWidth(char);
  return total;
}

/** Longest prefix of `text` that fits in `width` cells without splitting a character. */
export function truncateToWidth(text: string, width: number): string {
  if (width <= 0) return "";
  let used = 0;
  let output = "";
  for (const char of text) {
    const next = charWidth(char);
    if (used + next > width) break;
    output += char;
    used += next;
  }
  return output;
}
