# How Lite is tested

Herdr Annotate Lite is one native Rust binary. Its behavior was defined by the Bun runtime this
repository shipped through 0.3.0, and that definition is still enforced, by recording rather than
by running both runtimes side by side.

## Recorded goldens

`scripts/lite-regression.py` drives all six entrypoints the Lite manifest declares and compares
every observable with a recording checked in under `scripts/lite-goldens/`. The recording was taken
from the Bun runtime at commit `2add0da`, the last commit that still contained `src/`, so a green
run means the native runtime still does what Bun did.

```sh
bash scripts/lite-regression.sh
```

The wrapper builds and stages a release binary, then checks it. `rust-lite-ci` runs the same
command on Ubuntu and macOS.

### What one run compares

- Process cases: exit code, stdout, stderr, every fake `herdr` and clipboard call with its
  arguments, notification arguments, clipboard bytes, pending bytes and mode, and the resulting
  filesystem tree.
- Screen cases: real PTYs at 86x22 (editor) and 98x28 (manager). The ANSI model in the harness
  ignores style escapes and retains the terminal cell grid, including wide-character continuation
  cells. It snapshots the first frame and the frame after every input, and compares every OSC 52
  clipboard sequence the pane emitted, in order. Where a case compares clipboard bytes, the last
  sequence's payload must equal the bytes the native writer was handed.
- Store cases: JSONL bytes, file modes, and leftover lock and temporary files after scripted editor
  and manager mutations.
- `store.cross-read`: exports `scripts/lite-goldens/fixtures/bun-editor-annotations.jsonl`, the
  exact record the Bun editor wrote in the recorded run, so the on-disk format stays readable.
- Manifests: `lite/herdr-plugin.toml` and the root Full manifest must agree on the header and on
  every action and pane declaration, and neither may gate the annotation tools to a platform.
- Error catalog: every user-visible message the Bun runtime produced still exists in the native
  source file that owns it.
- Key coverage: the run fails unless it sent every editor and manager key in the required set.

The only normalized values are each run's temporary roots, the repository root, the per-user
handoff directory, generated UUIDs, generated ISO timestamps, and the pid and time components of
pending and temporary filenames. Seeded timestamps stay literal. Screen cells and clipboard bytes
are never normalized.

### Clipboard adapters

One clipboard operation walks the platform's candidate list until an adapter works, so the goldens
record the role (`clipboard-read`, `clipboard-write`) instead of the program and hold on every
platform. The adapters themselves, their arguments, and the fallback order are checked against
`CLIPBOARD_CANDIDATES` in the harness, which also requires a run to exercise both a first-adapter
success and the full chain. macOS therefore covers `pbpaste` and `pbcopy`, and Ubuntu covers
Wayland, then xclip, then xsel.

### Re-recording

`bash scripts/lite-regression.sh --record` rewrites the goldens from the current binary. Do that
only for a deliberate behavior change, and read the diff: every rewritten line is a behavior the
retired runtime no longer defines.

## Everything else

| Check | Command |
|---|---|
| Unit and subprocess tests | `cargo test --manifest-path rust/Cargo.toml` |
| Lints and formatting | `cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings`, `cargo fmt --manifest-path rust/Cargo.toml --check` |
| Manifest structure, including the Windows gates | `python3 scripts/test-windows-full-manifest.py` |
| Binary fetchers | `bash scripts/test-fetch-herdr-annotate.sh`, `bash scripts/test-fetch-plannotator-tui.sh`, and the `.ps1` cases in `windows-full-ci` |
| Install, upgrade, swap, and both panes in a live session | `HERDR_SESSION=<disposable session> bash scripts/smoke.sh` |

## What retiring Bun cost

The old harness ran Bun and Rust on every case in the same run, so a divergence surfaced the moment
either side moved. The comparison is now against a fixed recording: it still catches a native
regression, but it can no longer settle a *new* behavior against a live Bun reference, and a
deliberate re-record is a human decision instead of a diff against a second implementation.

`docs/rust-lite-parity.md` and `docs/rust-lite-parity-proof.md` mapped the TypeScript call paths to
the Rust ones function by function. They are retired with the runtime they describe; read them at
commit `2add0da`.

Windows stays outside the PTY harness. It is covered by the Rust tests, the manifest and fetcher
checks in `windows-full-ci`, and the manual checklist in `windows-full-acceptance.md`.
