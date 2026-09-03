/**
 * Terminal cell widths.
 *
 * East Asian wide and fullwidth characters occupy two terminal cells. Counting
 * them as one shifts every column the editor computes, which misplaces the
 * cursor and lets IME preedit overlays land inside existing text.
 */


const GRAPHEME_SEGMENTER = new Intl.Segmenter("en", { granularity: "grapheme" });

export function* graphemes(text: string): IterableIterator<string> {
  for (const entry of GRAPHEME_SEGMENTER.segment(text)) yield entry.segment;
}

/** Cells occupied by a single character. Control characters count as zero. */
export function charWidth(char: string): number {
  return Bun.stringWidth(char);
}

/** Cells occupied by a string. */
export function stringWidth(text: string): number {
  return Bun.stringWidth(text);
}

/** Longest prefix of `text` that fits in `width` cells without splitting a character. */
export function truncateToWidth(text: string, width: number): string {
  if (width <= 0) return "";
  let used = 0;
  let output = "";
  for (const grapheme of graphemes(text)) {
    const next = stringWidth(grapheme);
    if (used + next > width) break;
    output += grapheme;
    used += next;
  }
  return output;
}
