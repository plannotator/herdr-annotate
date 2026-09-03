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

export class SgrMouseDecoder {
  private buffer = "";

  feed(input: string): SgrMouseInput {
    if (!input && !this.buffer) return { intercepted: false, reports: [] };
    const prefix = "\x1b[<";
    const data = this.buffer + input;
    this.buffer = "";
    const reports: SgrMouseReport[] = [];
    let intercepted = false;
    let offset = 0;

    while (offset < data.length) {
      const start = data.indexOf(prefix, offset);
      if (start < 0) break;
      intercepted = true;
      let end = start + prefix.length;
      while (end < data.length && /[0-9;]/u.test(data[end] ?? "")) end += 1;
      if (end === data.length) {
        this.buffer = data.slice(start);
        break;
      }
      const final = data[end];
      if (final !== "M" && final !== "m") {
        offset = end + 1;
        continue;
      }
      const parts = data.slice(start + prefix.length, end).split(";");
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
