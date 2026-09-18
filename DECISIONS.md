# DECISIONS — herdr-annotate multi-machine

Started: 2026-09-18T08:17:43Z
Updated: 2026-09-18T08:33:00Z
Worktree: /Users/emo/.herdr/worktrees/herdr-annotate/p0-annotate
Branch: p0/annotate-multimachine
Workspace: w3R (p0-annotate)

## Constraints
- No merge tonight. Morning review by human owner.
- Own this worktree only (`/Users/emo/.herdr/worktrees/herdr-annotate/p0-annotate`).
- No focus stealing in Herdr.

## Decisions

### D1: Route Global Copy via Herdr Client Clipboard API
In remote or multi-machine sessions (over SSH, `herdr --remote`, or federated machines), the plugin runs on the remote server host where no local display or GUI clipboard manager is available (e.g., headless Linux boxes without `xclip`/`wl-clipboard`). Furthermore, even if a local clipboard tool existed on the remote host, it would only write to the remote server's clipboard, not the viewing developer's client clipboard.
- We implement `write_client_clipboard(text: &str)` in `rust/src/herdr.rs` calling `herdr clipboard set --stdin`. Herdr server forwards this over WebSocket to the active viewing client (`ServerMessage::Clipboard { data }`).
- `copy-context` and `copy-archive` in `rust/src/cli.rs` invoke `write_client_clipboard` to route Markdown annotations directly to the client's clipboard.

### D2: Selection Retention across Mouse-up and Prefix in Herdr 0.9.0
In Herdr core (commit `0a4f3e5f` merged into `2c854568` and deployed across fleet machines `studio`, `mbp-16-m4`, `spark0`, `spark1`, `emo-win`), selection clearing was updated so mouse-up and prefix key (`Ctrl+B`) do not prematurely wipe terminal selection. The client context JSON (`HERDR_PLUGIN_CONTEXT_JSON`) provides `selected_text` directly to `capture`.
- We update plugin documentation and remote session guidance in `README.md` to reflect that Herdr 0.9.0 natively supports remote selection retention.

### D3: Test Harness Updates for stdin-consuming Fake Herdr
The existing integration test harness in `rust/tests/commands.rs` used a shell script mock for `herdr` that did not consume standard input. Because `herdr clipboard set --stdin` pipes clipboard text into stdin of the child process, a non-consuming mock can cause pipe write errors (EPIPE) if closed early.
- We update `fake_herdr` in `rust/tests/commands.rs` to consume `cat > /dev/null` before logging arguments and exiting.
- We add dedicated regression tests for `copy-archive` verifying active annotation retention on failure and archiving on success.

## Tradeoffs

- **Client clipboard vs OSC 52:** OSC 52 can only be emitted from an active terminal pane with direct PTY output. Global plugin actions (`annotate.copy-context`, `annotate.copy-archive`) run out-of-band without an attached PTY window. Herdr's RPC endpoint (`herdr clipboard set --stdin`) is therefore the only reliable route for global copy actions.
- **Fail-open vs strict error reporting:** On global copy failure, reporting an error notification and retaining unarchived records prevents data loss rather than silently writing to a disconnected or headless server's OS clipboard.

## Verification Evidence

- `cargo test --test commands`:
  6 passed (1 suite, 0.98s):
  - `commands::capture_persists_selection_and_context_then_opens_the_editor`
  - `commands::failed_editor_open_removes_the_pending_file_and_reports_failure`
  - `commands::copy_context_with_an_empty_store_notifies_and_succeeds`
  - `commands::manage_opens_the_manager_pane_with_the_typescript_arguments`
  - `commands::copy_archive_failure_preserves_active_annotations_and_does_not_archive`
  - `commands::copy_archive_success_archives_and_clears_active`
- `cargo test --lib`:
  80 passed (1 suite, 0.05s) covering formatting, store, archive workflow, handoff, and CLI routing.
- `cargo build --release && cargo test`:
  86 passed across all 4 suites (0.00s execution).

## Remaining Gaps / Next Steps
- Production prebuilt binaries in `bin/` (`herdr-annotate.exe` across all target architectures) will need to be re-staged when a release tag is cut.
- End-to-end live testing against a live SSH / multi-machine Herdr connection (client on Mac, server on spark0/linux) to observe full roundtrip to system clipboard outside simulated test harnesses.
