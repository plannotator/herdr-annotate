# GOAL — multi-machine herdr-annotate

Written: 2026-09-18T08:17:43Z
Updated: 2026-09-18T20:32:00Z
Worktree: /Users/emo/.herdr/worktrees/herdr-annotate/annotate-clipboard
Repo: /Users/emo/dev/herdr-remote-annotation/herdr-annotate
Branch: annotate-clipboard
Status: **PROPOSAL — awaiting operator approval of Done / Acceptance / Validation. Not permission to merge or close.**

## Objective
Global copy actions (`copy-context`, `copy-archive`) on a remote/multi-machine Herdr server deliver formatted Markdown to the **viewing client's** clipboard via `herdr clipboard set --stdin`. No second clipboard transport.

## Done criteria (proposal)

Specific, this worktree. Operator must approve before any of these count as finished.

1. `write_client_clipboard` in `rust/src/herdr.rs` is the only global-copy writer and invokes `herdr clipboard set --stdin` (stdin = Markdown body).
2. `copy-context` and `copy-archive` in `rust/src/cli.rs` call `write_client_clipboard`. They do not call `write_clipboard` / `pbcopy` / `xclip` / OSC 52.
3. `rust/tests/commands.rs` asserts both actions emit argv `clipboard set --stdin` and pipe Markdown containing `# Annotated context`.
4. Copy-archive failure (`herdr clipboard set` nonzero) leaves `annotations.jsonl` unchanged and does not write `archives.jsonl`.
5. `DECISIONS.md` records: exclusive path, min Herdr gate, no second transport, fleet-version blocker.
6. `README.md` documents that global copies use Herdr's client clipboard API, not the server OS clipboard.
7. No merge, no Linear close, no second transport invented while waiting on fleet Herdr.

## Acceptance criteria (proposal)

Observable product behavior once Herdr on the **plugin host** has `herdr clipboard set --stdin`.

1. On a remote or saved-machine workspace, `copy-context` puts the formatted annotation Markdown on the **viewing client's** clipboard. The server host clipboard is not the destination.
2. `copy-archive` does the same, then archives and clears active annotations **only** after clipboard set succeeds.
3. Empty store: `copy-context` / `copy-archive` notify "No annotations" and succeed without calling clipboard set.
4. Clipboard set failure: user-visible error notification; active annotations retained.
5. Manager pane copies (`y` / `c` / `Shift+C`) stay on the existing OSC 52 + native pane path (unchanged by this work).
6. Capture still reads `selected_text` from `HERDR_PLUGIN_CONTEXT_JSON` (Herdr selection retention). Headless `herdr clipboard get` is **out of scope**.
7. Tagged public Herdr 0.9.0 without `clipboard set` is insufficient. Gate: Herdr 0.9.0 **with** `herdr clipboard set --stdin` on the server. Stock 0.9.0 is not enough.

## Validation criteria (proposal)

How we prove the lists above. Operator must approve these checks.

1. `cargo test` in `rust/` passes (currently 87 tests / 4 suites), including:
   - `copy_context_routes_markdown_through_herdr_clipboard_set`
   - `copy_archive_success_archives_and_clears_active` (argv + stdin Markdown)
   - `copy_archive_failure_preserves_active_annotations_and_does_not_archive`
   - empty-store copy-context notify/success
2. Source grep: `copy_context` / `copy_archive` have no `write_clipboard(` call.
3. Command-center Mac: `herdr clipboard set --help` exists. This is the **client** probe, not a fleet-server probe.
4. Live round-trip (blocked until fleet Herdr is upgraded; not claimed met):
   - Plugin host is a fleet box (`spark0` or `spark1`) whose `herdr clipboard --help` lists `set`.
   - From a viewing Mac, invoke `copy-context` on that machine's workspace.
   - Viewing Mac system clipboard contains `# Annotated context` and the annotation body.
5. Until (4) is run, this worktree is **not finished**, even if (1)–(3) pass.

## Non-goals
- No merge to master or release deployment without operator-approved criteria **and** those criteria met.
- Do not close user Herdr panes or steal Herdr focus.
- Do not invent a second clipboard transport (VPN sink, OSC 52 for global actions, silent local-OS fallback).
- Do not upgrade/deploy fleet `~/.local/bin/herdr` from this plugin worktree unless the operator assigns that.
- Do not close Linear tickets.

## Current evidence (not a finish)
- Plugin path landed: `5866499`, tests `ca9e446`, docs `6b15b61` / `cf367bf` / `7e1b8aa`.
- Fleet `studio` / `spark0` / `spark1` / `emo-win` still lack `herdr clipboard set`. Live validation item (4) is unmet.
- `standup: … done` is a status ping only. It is not permission to merge or close.
