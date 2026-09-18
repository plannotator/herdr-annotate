# GOAL — multi-machine herdr-annotate

Written: 2026-09-18T08:17:43Z
Updated: 2026-09-18T08:25:00Z
Worktree: /Users/emo/.herdr/worktrees/herdr-annotate/p0-annotate
Repo: /Users/emo/dev/herdr-remote-annotation/herdr-annotate
Workspace: w3R (p0-annotate)

## Objective
Enable seamless multi-machine Herdr Annotate functionality across remote/headless servers and client machines:
1. Ensure global copy operations (`copy-context`, `copy-archive`) deliver formatted Markdown annotations to the connected viewing client's clipboard via Herdr's client clipboard API (`herdr clipboard set --stdin`), falling back gracefully to local OS clipboard when appropriate.
2. Align documentation and tests with the multi-machine selection retention and clipboard architecture deployed across the Herdr fleet (Herdr 0.9.0+).
3. Validate with ticket-scoped unit tests in `rust/tests/commands.rs` and `rust/src/`.

## Acceptance
- Working client clipboard routing in `herdr-annotate` via `herdr clipboard set --stdin` with local fallback.
- Unit and integration tests passing for command dispatch, copy-context, copy-archive, and failure handling.
- `GOAL.md` and `DECISIONS.md` fully recorded.
- No merge tonight; draft PR prepared if appropriate.

## Non-goals
- No merge to master or release deployment tonight.
- Do not close user Herdr panes or steal Herdr focus (`--no-focus` only).
- Do not invent or alter unrelated server protocols.
