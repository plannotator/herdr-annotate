import type { Annotation } from "./types";
import { graphemes, stringWidth } from "./width";

/** Remove terminal control characters while retaining useful whitespace. */
export function sanitizeTerminalText(text: string): string {
  return text
    .replace(/\t/g, "    ")
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, "");
}

/** Wrap plain text to terminal-cell-width lines while preserving explicit newlines. */
export function wrapText(text: string, width: number): string[] {
  const safeWidth = Math.max(1, width);
  const output: string[] = [];
  for (const sourceLine of text.replace(/\r\n/g, "\n").split("\n")) {
    if (sourceLine.length === 0) {
      output.push("");
      continue;
    }
    let line = "";
    let used = 0;
    for (const grapheme of graphemes(sourceLine)) {
      const width = stringWidth(grapheme);
      if (used + width > safeWidth && line !== "") {
        output.push(line);
        line = "";
        used = 0;
      }
      line += grapheme;
      used += width;
    }
    output.push(line);
  }
  return output;
}

function fenceFor(text: string): string {
  const longest = Math.max(0, ...Array.from(text.matchAll(/`+/g), (match) => match[0].length));
  return "`".repeat(Math.max(3, longest + 1));
}

/** Format saved annotations as portable, agent-neutral Markdown context. */
export function formatAnnotations(annotations: readonly Annotation[]): string {
  const sections = annotations.map((annotation, index) => {
    const source = [annotation.context.workspace_label, annotation.context.tab_label]
      .filter(Boolean)
      .join(" / ");
    const fence = fenceFor(annotation.selectedText);
    const metadata = source ? `\nSource: ${source}\n` : "";
    return [
      `## Annotation ${index + 1}`,
      metadata,
      "Selected text:",
      "",
      fence,
      annotation.selectedText,
      fence,
      "",
      "Comment:",
      "",
      annotation.comment,
    ]
      .filter((line, lineIndex, lines) => !(line === "" && lines[lineIndex - 1] === ""))
      .join("\n");
  });
  return `# Annotated context\n\n${sections.join("\n\n")}\n`;
}
