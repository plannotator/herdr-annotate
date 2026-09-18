# DECISIONS — herdr-annotate multi-machine

Started: 2026-09-18T08:17:43Z
Updated: 2026-09-18T20:58:00Z
Worktree: /Users/emo/.herdr/worktrees/herdr-annotate/annotate-clipboard
Branch: annotate-clipboard
Workspace: annotate-clipboard

## Constraints
- No merge tonight. Morning review by human owner.
- Own this worktree only (`/Users/emo/.herdr/worktrees/herdr-annotate/annotate-clipboard`).
- No focus stealing in Herdr.
- Do not invent a second clipboard transport.

## Decisions

### D1: Route Global Copy via Herdr Client Clipboard API
In remote or multi-machine sessions (over SSH, `herdr --remote`, or federated machines), the plugin runs on the remote server host where no local display or GUI clipboard manager is available (e.g., headless Linux boxes without `xclip`/`wl-clipboard`). Furthermore, even if a local clipboard tool existed on the remote host, it would only write to the remote server's clipboard, not the viewing developer's client clipboard.
- Plugin path (exclusive): `write_client_clipboard(text: &str)` in `rust/src/herdr.rs` calls `herdr clipboard set --stdin`. That is the only global-copy transport. Do not add OSC 52, VPN sinks, or local `pbcopy`/`xclip` fallbacks for `copy-context` / `copy-archive`.
- Herdr forwards the payload to the connected foreground viewing client via `client.clipboard.set` (`{ delivered: true }` means the client connection accepted it, not that the host OS clipboard acknowledged it).
- `copy-context` and `copy-archive` in `rust/src/cli.rs` invoke `write_client_clipboard` to route Markdown annotations to that client clipboard.

### D1a: Minimum Herdr version for `clipboard set`
- Tagged public Herdr **0.9.0** (2026-09-07) does **not** include `herdr clipboard set`. The command and `client.clipboard.set` landed in herdr-core **Unreleased** after that tag (commit `0a4f3e5f`, merged in `2c854568`).
- Practical gate: Herdr **0.9.0 with `herdr clipboard set --stdin`** (command-center Mac build has this). Stock 0.9.0 without the annotation patches is insufficient.
- There is no later public semver that ships the command yet. Probe: `herdr clipboard set --stdin` must exist on the **server** host where the plugin runs.

### D2: Selection Retention across Mouse-up and Prefix in Herdr 0.9.0
In Herdr core (commit `0a4f3e5f` merged into `2c854568` and deployed across fleet machines `studio`, `mbp-16-m4`, `spark0`, `spark1`, `emo-win`), selection clearing was updated so mouse-up and prefix key (`Ctrl+B`) do not prematurely wipe terminal selection. The client context JSON (`HERDR_PLUGIN_CONTEXT_JSON`) provides `selected_text` directly to `capture`.
- We update plugin documentation and remote session guidance in `README.md` to reflect that Herdr 0.9.0 natively supports remote selection retention.

### D3: Test Harness Updates for stdin-consuming Fake Herdr
The existing integration test harness in `rust/tests/commands.rs` used a shell script mock for `herdr` that did not consume standard input. Because `herdr clipboard set --stdin` pipes clipboard text into stdin of the child process, a non-consuming mock can cause pipe write errors (EPIPE) if closed early.
- We update `fake_herdr` in `rust/tests/commands.rs` to consume stdin (now captured to `HERDR_TEST_STDIN`) before logging arguments and exiting.
- We add dedicated regression tests for `copy-archive` verifying active annotation retention on failure and archiving on success, plus `copy_context_routes_markdown_through_herdr_clipboard_set` proving the `clipboard set --stdin` argv and Markdown payload.

### D4: No second clipboard transport
Global copies stay on `herdr clipboard set --stdin` only. OSC 52 remains the pane-manager path (it needs a PTY). Do not add VPN clipboard sinks, extra RPC, or silent local-OS fallbacks for `copy-context` / `copy-archive`. Wait for fleet Herdr to grow `clipboard set`.

## Tradeoffs

- **Client clipboard vs OSC 52:** OSC 52 can only be emitted from an active terminal pane with direct PTY output. Global plugin actions (`annotate.copy-context`, `annotate.copy-archive`) run out-of-band without an attached PTY window. Herdr's RPC endpoint (`herdr clipboard set --stdin`) is therefore the only reliable route for global copy actions.
- **Fail-open vs strict error reporting:** On global copy failure, reporting an error notification and retaining unarchived records prevents data loss rather than silently writing to a disconnected or headless server's OS clipboard.

## Verification Evidence

- `cargo test --test commands`:
  7 passed, including `copy_context_routes_markdown_through_herdr_clipboard_set` and copy-archive stdin routing through `clipboard set --stdin`.
- `cargo test --lib`:
  80 passed covering formatting, store, archive workflow, handoff, and CLI routing.
- `cargo test` after the clipboard-path tests:
  87 passed across 4 suites.

## Remaining Gaps / Next Steps
- Plugin path is landed and locally proven. Fleet Herdr on spark0/spark1/mbp-16-24 now exposes `herdr clipboard set --stdin`.
- Local viewing-client round-trip succeeded 2026-09-18T20:58Z (`copy-context` → Mac clipboard `# Annotated context`).
- Remote plugin-host round-trip still unmet: spark0 CLI invoke returns `no_foreground_client` unless a TUI client is viewing that machine. Command-center Herdr has no `--machine` flag, so saved-machine plugin actions cannot be targeted from this Mac's CLI. Do not steal focus to attach a viewer.
- `emo-win` SSH timed out. Do not invent a second clipboard transport.
