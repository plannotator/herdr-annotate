import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dir, "..");
const outputDirectory = path.join(root, "bin");
const output = path.join(outputDirectory, "herdr-annotate.exe");
const entrypoint = path.join(root, "src", "runtime.ts");

fs.mkdirSync(outputDirectory, { recursive: true });

const build = Bun.spawnSync({
  cmd: [
    process.execPath,
    "build",
    "--compile",
    "--reject-unresolved",
    "--no-compile-autoload-dotenv",
    "--no-compile-autoload-bunfig",
    "--outfile",
    output,
    entrypoint,
  ],
  cwd: root,
  stdout: "inherit",
  stderr: "inherit",
});

if (!build.success) process.exit(build.exitCode);
