type RuntimeCommand = "capture" | "copy-context" | "editor" | "manage" | "manager";

export {};

function parseRuntimeCommand(value: string | undefined): RuntimeCommand | undefined {
  switch (value) {
    case "capture":
    case "copy-context":
    case "editor":
    case "manage":
    case "manager":
      return value;
    default:
      return undefined;
  }
}

const command = parseRuntimeCommand(process.argv[2]);

switch (command) {
  case "capture":
    await import("./capture");
    break;
  case "copy-context":
    await import("./export");
    break;
  case "editor":
    await import("./editor");
    break;
  case "manage":
    await import("./open-manager");
    break;
  case "manager":
    await import("./manager");
    break;
  default:
    console.error(
      "Usage: herdr-annotate.exe <capture|copy-context|editor|manage|manager>",
    );
    process.exit(2);
}
