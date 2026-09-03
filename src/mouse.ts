export type SgrMouseReport = {
  button: number;
  x: number;
  y: number;
  action: "press" | "release";
};

export type SgrMouseInput = {
  intercepted: boolean;
  reports: SgrMouseReport[];
};

const SGR_PREFIX = "\x1b[<";
const LEGACY_PREFIX = "\x1b[M";
const MAX_SGR_REPORT_LENGTH = 64;

export function isLeftMousePress(report: SgrMouseReport): boolean {
  return (
    report.action === "press" &&
    Number.isInteger(report.button) &&
    report.button >= 0 &&
    report.button <= 28 &&
    (report.button & 3) === 0
  );
}

export class SgrMouseDecoder {
  private buffer = "";
  private discardingSgr = false;

  feed(input: string): SgrMouseInput {
    if (!input && !this.buffer && !this.discardingSgr) {
      return { intercepted: false, reports: [] };
    }
    if (this.discardingSgr) {
      let end = 0;
      while (end < input.length && /[0-9;]/u.test(input[end] ?? "")) end += 1;
      if (end === input.length) return { intercepted: true, reports: [] };
      this.discardingSgr = false;
      return { intercepted: true, reports: [] };
    }
    const data = this.buffer + input;
    this.buffer = "";
    const reports: SgrMouseReport[] = [];
    let intercepted = false;
    let offset = 0;

    while (offset < data.length) {
      const sgrStart = data.indexOf(SGR_PREFIX, offset);
      const legacyStart = data.indexOf(LEGACY_PREFIX, offset);
      if (sgrStart < 0 && legacyStart < 0) break;
      const legacy =
        legacyStart >= 0 && (sgrStart < 0 || legacyStart < sgrStart);
      const start = legacy ? legacyStart : sgrStart;
      intercepted = true;

      if (legacy) {
        const end = start + 6;
        if (end > data.length) {
          this.buffer = data.slice(start);
          break;
        }
        const button = data.charCodeAt(start + 3) - 32;
        const x = data.charCodeAt(start + 4) - 32;
        const y = data.charCodeAt(start + 5) - 32;
        if (button >= 0 && x > 0 && y > 0) {
          reports.push({
            action: (button & 3) === 3 ? "release" : "press",
            button,
            x,
            y,
          });
        }
        offset = end;
        continue;
      }

      let end = start + SGR_PREFIX.length;
      while (end < data.length && /[0-9;]/u.test(data[end] ?? "")) end += 1;
      if (end === data.length) {
        const candidate = data.slice(start);
        if (candidate.length <= MAX_SGR_REPORT_LENGTH) {
          this.buffer = candidate;
        } else {
          this.discardingSgr = true;
        }
        break;
      }
      const final = data[end];
      if (final !== "M" && final !== "m") {
        offset = end + 1;
        continue;
      }
      const parts = data.slice(start + SGR_PREFIX.length, end).split(";");
      if (parts.length !== 3 || parts.some((part) => part.length === 0)) {
        offset = end + 1;
        continue;
      }
      const values = parts.map((part) => Number(part));
      if (values.some((value) => !Number.isInteger(value) || value < 0)) {
        offset = end + 1;
        continue;
      }
      const [button, x, y] = values;
      if (button === undefined || x === undefined || y === undefined) {
        offset = end + 1;
        continue;
      }
      reports.push({ button, x, y, action: final === "M" ? "press" : "release" });
      offset = end + 1;
    }

    return { intercepted, reports };
  }
}
