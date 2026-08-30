# Rust Lite parity checklist

This document maps the TypeScript Lite implementation at `origin/main` to the native evaluation
crate. Checked items have a Rust implementation and automated coverage. Live checks are recorded
separately so test coverage is not confused with behavior observed inside Herdr.

## Entrypoints and plugin contract

- [x] `capture.ts` → `herdr-annotate capture`: selection precedence is invocation context, fresh
  one-shot handoff file, then platform clipboard; blank selections notify without failing; pending
  JSON is mode 0600; pane-open failure removes it.
- [x] `export.ts` → `herdr-annotate copy-context`: loads newest first, formats Markdown, copies, and
  sends the same singular/plural notifications and errors.
- [x] `open-manager.ts` → `herdr-annotate manage`: requires `HERDR_PLUGIN_ROOT` and opens the same
  focused 100×30 popup.
- [x] `editor.ts` → `herdr-annotate editor`: pending-file and invocation-context fallback, delete-on-
  successful-read behavior, multiline Unicode editing, wide-cell cursor layout, Ctrl+S validation,
  save delay, Esc/Ctrl+C cancellation, raw terminal restoration.
- [x] `manager.ts` → `herdr-annotate manager`: active/archive views, newest-first lists, detail panes,
  navigation, copy one/all, copy-and-archive, delete, double-confirm clear, restore, double-confirm
  permanent archive deletion, reload, status messages, Esc/Tab/q/Ctrl+C behavior.
- [x] `lite-rs/herdr-plugin.toml` preserves plugin id `annotate`, action ids, pane ids, placements,
  dimensions, contexts, and supported platform declarations. All commands are the one native binary.

## Modules and compatibility boundaries

- [x] `types.ts`: retained Herdr fields, permissive context projection, required pending fields,
  non-empty saved fields, complete version-1 archive validation, unknown-field tolerance.
- [x] `paths.ts`: state/root environment variables, Windows extended drive and UNC normalization,
  `annotations.jsonl` and `archives.jsonl` names.
- [x] `store.ts`: append-order JSONL, exact camelCase field names and distinct TypeScript pending and
  saved-record field orders, trailing newline, mode 0600, whole-store invalid-data rejection, ID
  merge/remove, atomic temporary replace, per-store directory locks, owner tokens, 30-second stale
  recovery, ownership-checked release.
- [x] `format.ts`: control sanitization, four-space tabs, CRLF normalization, explicit-newline and
  terminal-cell wrapping, Markdown headings/source/fences/blank-line shape, safe longer backtick fence.
- [x] `width.ts` and `layout.ts`: the same wide ranges, zero-width controls/combining marks, non-
  splitting truncation/wrapping, terminal-cell cursor coordinates.
- [x] `handoff.ts`: per-user runtime/temp path, 15-second freshness, read-once removal, blank rejection.
- [x] `clipboard.ts`: `pbpaste`/`pbcopy`; PowerShell raw get/set; Linux Wayland then xclip then xsel.
- [x] `herdr.ts`: `HERDR_BIN_PATH` override, stderr projection, best-effort notifications.
- [x] `archive-workflow.ts` and `manager-copy.ts`: operation order and partial-failure states, including
  preserving a concurrently saved annotation.

`plannotator-tui-schema` is intentionally not a dependency: its `Annotation` is the Full product's
document-anchor/API wire shape, not Lite's existing terminal-selection JSONL shape.

## TypeScript test mapping

| TypeScript spec | Rust counterpart |
|---|---|
| `test/types.test.ts` | `types::tests` (8 tests, including JSON-number and JS-trim edges) |
| `test/paths.test.ts` | `paths::tests` (2 tests; all five TS path cases covered) |
| `test/handoff.test.ts` | `handoff::tests` (4 tests, including future-clock skew) |
| `test/format.test.ts` | `format::tests` plus malformed-boundary store/type tests |
| `test/width.test.ts` | `width::tests` (3 grouped tests covering every assertion) |
| `test/layout.test.ts` | `layout::tests` (3 grouped tests covering every assertion) |
| `test/store.test.ts` | `store::tests` (6 tests) |
| `test/archive-workflow.test.ts` | `archive_workflow::tests` (8 tests) |
| `test/manager-copy.test.ts` | `manager_copy::tests` (3 tests) |

Additional Rust-only coverage:

- `editor::tests`: headless 86×22 `TestBackend` frame, Unicode edit keys, empty-save validation, quit.
- `manager::tests`: headless 98×28 active/archive frames, newest-first detail, exact TypeScript detail
  width at the clipping boundary, confirmation, real clear.
- `rust/tests/commands.rs`: subprocess-level capture/manage/copy-context commands with a fake Herdr,
  including exact pane argv, pending JSON, notifications, and pending cleanup on pane-open failure.
- `types::tests::serialization_matches_the_typescript_pending_and_saved_field_order`: literal pending
  and saved JSON byte shapes.

## Distribution and verification

- [x] Rust 2024, Rust 1.96, repository lint policy, pedantic Clippy with `-D warnings`, no production
  `unwrap()`, release LTO/strip, and `cargo fmt`.
- [x] Checksummed release assets for macOS (Intel/Apple Silicon), Linux (x86_64/aarch64), and Windows
  are defined by `.github/workflows/rust-lite-release.yml`.
- [x] Unix and PowerShell installers verify `SHA256SUMS`; `lite-rs/scripts/stage-local.sh` builds and
  stages the binary before `herdr plugin link` (link intentionally does not run manifest build hooks).
- [x] `scripts/smoke-rust-lite.sh` refuses the default session, preserves/restores the globally
  installed `annotate` plugin, checks the native manifest/binary, and renders/closes the manager
  entrypoint through Herdr.
- [ ] Release download mode is defined and tested structurally, but cannot be exercised before an
  unmerged evaluation tag/release exists.
- [ ] Windows behavior is covered by platform-specific code and Windows CI, but was not live-tested
  on this macOS development host.

## Live verification

On 2026-08-29, `scripts/smoke-rust-lite.sh` completed with zero failures against only the disposable
`rust-lite-test` session on macOS. It built and staged the release binary, linked `lite-rs`, verified
all three action commands and the bundled version, rendered the native manager popup through Herdr,
and closed it. The previously installed GitHub `annotate` plugin was restored at its exact commit,
and the disposable session was stopped afterward.

The capture/editor save flow, system clipboard adapters, archive mutations, and every manager key
path were verified by automated unit, subprocess, and `TestBackend` tests rather than mutating live
annotation or clipboard data. Windows remains CI-only until it is exercised on a Windows host.

## Deliberate display-level difference

JavaScript's `Date.toLocaleString()` delegates to the host's full locale database. The Rust manager
uses the same local timezone but an en-US-style `M/D/YYYY, h:mm:ss AM/PM` string. Persisted timestamps,
sorting, and export do not change; only manager timestamp presentation can differ for non-en-US users.

No TypeScript source was changed.
