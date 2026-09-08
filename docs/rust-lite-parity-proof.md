# Rust Lite parity proof

This is the re-runnable proof behind the Rust Lite evaluation. It maps the externally observable
TypeScript Lite surface to the Rust call path at function granularity and names the differential
case that compares the result. The current local result is:

```text
Parity Lite: 510 observables compared, 94 screens diffed, zero divergences / 1 deliberate
```

Run it from the repository root with `bash scripts/parity-lite.sh`. The shell wrapper stages a fresh
release binary; `scripts/parity-lite.py` then runs the Bun entrypoints and the staged native binary
against separate copies of the same fixtures. A failure retains its temporary inputs and unified
diffs and prints their path. A green run removes its temporary workspace.

## What the harness compares

- Process cases compare exit code, stdout, stderr, every fake `herdr`/clipboard command and argument,
  notification arguments, clipboard bytes, pending bytes and mode, and resulting filesystem trees.
- Manifest cases parse `lite/herdr-plugin.toml`, `lite-rs/herdr-plugin.toml`, and the root Full
  manifest and compare the declared entrypoints field by field, so a new action cannot reach one
  runtime only.
- Screen cases use real PTYs at 86×22 (editor) and 98×28 (manager). The ANSI parser at
  `scripts/parity-lite.py:158` ignores style escapes but retains the terminal cell grid, including
  wide-character continuation cells. It snapshots the initial frame and the frame after every input.
- Screen cases also retain the raw PTY byte stream and compare every OSC 52 clipboard sequence a
  pane emitted, in order (`scripts/parity-lite.py:388`). Where a case compares clipboard bytes, the
  payload of the last sequence is additionally required to equal the bytes the native writer received,
  so a runtime cannot emit a different copy to the terminal than it wrote to the clipboard.
- Store cases byte-compare JSONL, modes, and leftover lock/temp files after scripted editor and
  manager mutations. `store.cross-read` makes Bun export the Rust editor's store and Rust export the
  Bun editor's store, then compares the Markdown and subprocess traces.
- The only normalized values are each case's deliberately different temporary root, generated UUIDs,
  generated ISO timestamps, and pid/time components in pending and temporary filenames
  (`scripts/parity-lite.py:481`). Seed timestamps are allow-listed and remain literal. Product files
  are never rewritten or filtered. Screen cells and clipboard bytes are never normalized.
- The harness runs on the host's real adapter branch. macOS therefore proves `pbpaste`/`pbcopy` and
  Ubuntu proves the Wayland → xclip → xsel chain. Windows is intentionally outside this PTY harness
  and remains in the separate build/promotion track.

The parity surface is the six commands wired by the Lite manifest. Rust's single-binary dispatch
usage error and `--version` output are packaging controls outside that surface; no valid manifest
invocation reaches them. Conversely, TypeScript dynamic-import loader failures have no native
counterpart because those modules are linked into the binary. Their underlying clipboard/store
failures do have mapped counterparts below.

## Entrypoints

### `capture`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest invocation | `lite/herdr-plugin.toml:17` starts `bun ../src/capture.ts`. | `lite-rs/herdr-plugin.toml:27` starts `herdr-annotate capture`; `rust/src/main.rs:3` passes argv to `cli::run` at `rust/src/cli.rs:33`, then `capture` at `rust/src/cli.rs:65`. | Every `process.capture.*` case. |
| Context decode | Top-level `src/capture.ts:12` parses `HERDR_PLUGIN_CONTEXT_JSON`; invalid JSON falls back to an empty object. `parseInvocationContext` and `selectedTextFromInvocation` are `src/types.ts:44` and `src/types.ts:58`. | `invocation_context` at `rust/src/cli.rs:58`, then `parse_invocation_context` and `selected_text_from_invocation` at `rust/src/types.rs:85` and `rust/src/types.rs:101`. | `process.capture.context`, `process.capture.invalid-context`. |
| Required paths | `stateDir`/`pluginRoot` at `src/paths.ts:27` and `src/paths.ts:32`, checked at `src/capture.ts:21`. | `state_dir`/`plugin_root` at `rust/src/paths.rs:29` and `rust/src/paths.rs:36`, checked at `rust/src/cli.rs:69`. | `process.capture.missing-state`, `process.capture.missing-root`. |
| Selection precedence | Invocation selection at `src/capture.ts:15`; if absent, `takeHandoff` at `src/handoff.ts:24`; if absent/blank/stale, `readClipboard` at `src/clipboard.ts:48`. | The same branches at `rust/src/cli.rs:71`, through `take_default_handoff`/`take_handoff` at `rust/src/handoff.rs:60`/`:32`, then `read_clipboard` at `rust/src/clipboard.rs:93`. | `process.capture.context` seeds all three sources and leaves the skipped handoff untouched; `process.capture.handoff` seeds a competing clipboard; stale, blank, invalid-UTF-8, and clipboard cases exercise the remaining decisions. |
| Empty selection | `src/capture.ts:38` sends `Nothing to annotate`, creates no pending file, exits 0. | `rust/src/cli.rs:78` sends the same notification and returns success. | `process.capture.empty`. |
| Pending record | `src/capture.ts:42` creates the state directory; `:44` constructs the record; `:49` names it; `:52` writes it. | `rust/src/cli.rs:85` constructs it; `:92` names it; `write_pending` at `:247` writes it. | All successful capture cases compare literal JSON after generated time/name normalization and assert mode 0600. |
| Editor pane | `runHerdr` at `src/herdr.ts:11` receives the argv built at `src/capture.ts:54`. Failure removes pending at `:74`, then the catch at `:78` notifies, prints, and exits 1. | `run_herdr` at `rust/src/herdr.rs:22` receives the argv at `rust/src/cli.rs:97`. Failure removes pending at `:117`; `cli::run` notifies at `:35`; `main` prints/exits 1 at `rust/src/main.rs:3`. | `process.capture.context` compares success argv; `process.capture.open-failure` compares cleanup, notification, stderr, and exit. |

The structural difference is exception flow versus `Result`. Both converge on the same process
contract: success and blank input exit 0; a defined failure produces one stderr line, one best-effort
`Annotate failed` notification, and exit 1.

### `copy-context`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest and state | `lite/herdr-plugin.toml:24` → top-level `src/export.ts:7`; state is required at `:8`. | `lite-rs/herdr-plugin.toml:34` → `rust/src/main.rs:3` → `cli::run` at `rust/src/cli.rs:33` → `copy_context` at `:124`. | Every `process.copy.*` case. |
| Load and ordering | `loadAnnotations` at `src/store.ts:37` locks/parses; `newestFirstAnnotations` at `:32` reverses a copy; `src/export.ts:10` propagates load failure. | `load_annotations` at `rust/src/store.rs:70` locks/parses; `newest_first_annotations` at `:65` reverses clones; `rust/src/cli.rs:126` propagates failure. | Empty, populated, invalid-store, busy-lock, and stale-lock cases. |
| Empty store | `src/export.ts:14` notifies `No annotations` / `There is nothing to copy yet.` and exits 0. | `rust/src/cli.rs:127` sends the same notification and returns success. | `process.copy.empty`, including the newly created state-directory mode. |
| Markdown and clipboard | `formatAnnotations` at `src/format.ts:44`, then `writeClipboard` at `src/clipboard.ts:63`. | `format_annotations` at `rust/src/format.rs:67`, then `write_clipboard` at `rust/src/clipboard.rs:110`. | `process.copy.single`, `process.copy.populated`, `process.copy.no-clipboard`, and `store.cross-read`; clipboard bytes are unnormalized. |
| Success/failure reporting | `src/export.ts:21` sends singular/plural `Annotations copied`; catch at `:25` sends `Copy failed`, prints, exits 1. | `rust/src/cli.rs:132` sends the same success notification; `cli::run` at `:38` sends `Copy failed`; `main` prints/exits 1. | Single and populated cases prove grammar; no-clipboard, invalid-store, busy-lock, and missing-state prove failure outputs and exits. |

### `copy-archive`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest and state | `lite/herdr-plugin.toml:31` → top-level `src/export-archive.ts:76` → `main` at `:45`; state is required at `:47`. | `lite-rs/herdr-plugin.toml:41` → `rust/src/main.rs:3` → `cli::run` at `rust/src/cli.rs:33` → `copy_archive` at `:183`; state is required at `:184`. | Every `process.copy-archive.*` case. |
| Shared workflow | `copyAndArchiveAnnotations` at `src/archive-workflow.ts:29` receives the same dependencies `src/manager.ts:228` injects, wired at `src/export-archive.ts:55`. | `copy_and_archive_annotations` at `rust/src/archive_workflow.rs:27` receives the dependencies `rust/src/manager.rs:556` injects, wired at `rust/src/cli.rs:191`. | `test/archive-workflow.test.ts` and `archive_workflow::tests` cover the workflow; `store.manager.copy-archive` covers the manager key; the process cases cover the action. |
| Operation order | Load, format newest first, clipboard write, `appendArchivedSet`, then `removeAnnotationsById` — `src/archive-workflow.ts:32`-`:52`. | The same order at `rust/src/archive_workflow.rs:38`-`:65`. | `process.copy-archive.populated` byte-compares both JSONL stores, modes, and leftover lock/temp files afterwards, plus the unnormalized clipboard bytes. |
| Empty store | The injected loader records the empty read at `src/export-archive.ts:58`; `copyArchiveReport` at `:39` notifies `No annotations` / `There is nothing to copy yet.` and exits 0, matching `export.ts`. | `Cell` flag at `rust/src/cli.rs:194`; `copy_archive_report` at `:170` sends the same notification and returns success. | `process.copy-archive.empty` compares the notification, exit 0, and the untouched state tree. |
| Success reporting | `copyArchiveReport` at `src/export-archive.ts:25` sends singular/plural `Annotations copied and archived`. | `copy_archive_report` at `rust/src/cli.rs:157` builds the same title and body; `copy_archive` notifies at `:207`. | `process.copy-archive.populated`; `test/export-archive.test.ts` and `cli::tests::copy_archive_maps_every_outcome_to_its_notification_and_exit_status` pin both grammars. |
| Failure reporting | A `stay_open` failure maps to `Copy and archive failed` at `src/export-archive.ts:42`; the retained-active partial maps to `Copy and archive incomplete` at `:32`. Both print the body and exit 1 at `:70`. | The same two arms at `rust/src/cli.rs:175` and `:165`; `copy_archive` returns the body at `:209` and `main` prints/exits 1. | `process.copy-archive.no-clipboard` and `process.copy-archive.missing-state` compare notification argv, stderr, exit code, and the unchanged stores; the retained-active arm is pinned by both unit tests. |

### `manage`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest and root | `lite/herdr-plugin.toml:38` → `src/open-manager.ts:5` → `pluginRoot` at `src/paths.ts:32`. | `lite-rs/herdr-plugin.toml:48` → `rust/src/main.rs:3` → `cli::run` at `rust/src/cli.rs:33` → `manage` at `:214` → `plugin_root` at `rust/src/paths.rs:36`. | `process.manage.success`, `process.manage.missing-root`. |
| Manager pane | The argv literal is `src/open-manager.ts:13`; `runHerdr` is `src/herdr.ts:11`. Errors notify/print/exit at `src/open-manager.ts:32`. | The argv literal is `rust/src/cli.rs:216`; `run_herdr` is `rust/src/herdr.rs:22`; `cli::run`/`main` handle notify, stderr, and exit. | Success, child-stderr failure, empty-child-stderr fallback, and missing-root cases. |

### `editor`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest and pending selection | `lite/herdr-plugin.toml:46` → top-level `src/editor.ts:17`. `invocationContext` at `:20`; pending-file parse at `:34`; fallback `pendingAnnotationFromInvocation` at `src/types.ts:65`; parsed-file canonicalization at `src/types.ts:79`. | `lite-rs/herdr-plugin.toml:56` → dispatcher → `editor::run` at `rust/src/editor.rs:346`; `pending_from_env` at `:320`; invocation fallback and pending parsing at `rust/src/types.rs:107` and `:119`. | Missing/invalid process cases; `screen.editor.pending-file-save`; the other editor PTY cases use invocation fallback. |
| Terminal and screen | `render` at `src/editor.ts:77` uses `sanitizeTerminalText`, `wrapText`, `layoutComment`, and width helpers; alternate-screen setup is `:207`. | `EditorApp::draw` at `rust/src/editor.rs:67` calls the corresponding helpers at `rust/src/format.rs:7`/`:26`, `rust/src/layout.rs:14`, and `rust/src/width.rs:35`; `editor::run` owns the Ratatui terminal. | Initial frame and every frame in `screen.editor.*`; fixed 86×22 cells include wide Hangul. |
| Input and save | Key dispatch is `src/editor.ts:172`; vertical movement is `:52`; `save` is `:122`; successful save renders, waits 250 ms, and exits. | `EditorApp::handle_key` at `rust/src/editor.rs:168`; vertical movement at `:227`; `save` at `:253`; `run` at `:346` renders the saved state, waits 250 ms, and exits. | `screen.editor.edit-save`, empty-save, missing-state, Esc, and Ctrl+C cases. Store bytes are compared after save. |
| Cleanup and signals | `cleanup`/`exit` at `src/editor.ts:110`/`:117`; SIGTERM/SIGHUP handlers at `:160`. | `Termination::install`/`requested` at `rust/src/termination.rs:17`/`:33`; the polling loop at `rust/src/editor.rs:346` reaches `ratatui::restore`. | `screen.editor.sigterm` compares exit 0 and the restored terminal grid. |

### `manager`

| Observable decision or effect | TypeScript call path | Rust call path | Mechanical evidence |
|---|---|---|---|
| Manifest, state, initial load | `lite/herdr-plugin.toml:54` → `requireStateDir` at `src/manager.ts:30`; `reloadActive`/`reloadArchives` at `:55`/`:66`. | `lite-rs/herdr-plugin.toml:64` → dispatcher → `manager::run` at `rust/src/manager.rs:724`; `ManagerApp::load` at `:63`; reload methods at `:80`/`:95`. | `process.manager.missing-state`; initial screens in all manager PTY cases. |
| Active screen | `render` at `src/manager.ts:198` → `renderActive` at `:95`, with `clipped` at `:77`, formatting helpers, newest-first state, source and timestamp metadata. | `ManagerApp::draw` at `rust/src/manager.rs:110` → `draw_active` at `:162`, with `clipped` at `:681` and `format_timestamp` at `:691`. | Every active-view snapshot, including the detail-width regression fixture in `screen.manager.all-views`. |
| Archive screen | `render` → `renderArchives` at `src/manager.ts:138`; archive annotations are previewed newest first. | `ManagerApp::draw` → `draw_archives` at `rust/src/manager.rs:300`; same preview ordering and clipping. | Every archive-view snapshot and scripted archive mutation. |
| Input and mutations | Top-level key dispatch is `src/manager.ts:404`; active/archive handlers are `:320`/`:346`; action functions are `:218`–`:318`. | `ManagerApp::handle_key` is `rust/src/manager.rs:434`; view handlers are `:466`/`:503`; action methods are `:547`–`:651`. | `screen.manager.all-views`, empty-actions, success-copy sessions, and the exit/signal sessions. Resulting JSONL and clipboard bytes are compared. |
| Cleanup and signals | `cleanup`/`exit` at `src/manager.ts:378`/`:385`; signal handlers at `:392`. | `Termination` at `rust/src/termination.rs:17`; polling/restore at `rust/src/manager.rs:724`. | `screen.manager.sighup` compares exit 0 and restored cells. |

## Every editor key

All key paths clear the prior status before acting. TypeScript dispatch is `src/editor.ts:172`;
Rust dispatch is `EditorApp::handle_key` at `rust/src/editor.rs:168`; both render again after the
transition.

| Key | TypeScript → Rust call path | Required state/store/screen effect | Harness step |
|---|---|---|---|
| Character input, including wide text | `src/editor.ts:199` uses `Array.from` and `splice` → `rust/src/editor.rs:210` calls `insert` at `:222`. | Insert Unicode scalar(s) at cursor, advance by character count, render using cell width. | `screen.editor.edit-save`: `chars`, `chars-second-line`. |
| Enter | `src/editor.ts:196` → Rust `KeyCode::Enter` at `rust/src/editor.rs:209` → `insert`. | Insert `\n`, move cursor, preserve explicit blank/line layout. | `enter`. |
| Backspace | `src/editor.ts:180` → `rust/src/editor.rs:182`. | If cursor > 0, remove the character before it and move left; otherwise no change. | `backspace`. |
| Delete | `src/editor.ts:182` → `rust/src/editor.rs:188`. | Remove the character at cursor if present; cursor stays. | `delete`. |
| Left / Right | `src/editor.ts:184`/`:186` → `rust/src/editor.rs:193`/`:194`. | Move one character, clamped to `[0, length]`. | `left`, `right`. |
| Up / Down | `moveCursorVertical` at `src/editor.ts:52` → `move_cursor_vertical` at `rust/src/editor.rs:227`. | Preserve terminal-cell column as closely as possible on the adjacent line; clamp first/last row and never split a wide glyph. | `up`, `down`. |
| Home / End | `src/editor.ts:192`/`:194` → `rust/src/editor.rs:197`/`:202`. | Move to start/end of the current logical line. | `home`, `end`. |
| Ctrl+S | `src/editor.ts:175` → `save` at `:122`; Rust control branch at `rust/src/editor.rs:177` → `save` at `:253`. | Blank comment: `Write a comment before saving.` and remain. Missing state: `Plugin state directory is unavailable.` and remain. Store error: display it and remain. Success: append exact JSONL, display `Saved.`, wait 250 ms, cleanly exit 0. | `screen.editor.edit-save`, `empty-save-escape`, `missing-state`. |
| Esc | `src/editor.ts:179` → `exit`/`cleanup`; Rust `rust/src/editor.rs:181` sets quit and `run` restores. | No store write; cursor shown, screen restored, exit 0. | `screen.editor.empty-save-escape`. |
| Ctrl+C | `src/editor.ts:174` → `exit`/`cleanup`; Rust `rust/src/editor.rs:173` sets quit. | Same cancellation effect as Esc. | `screen.editor.control-c`. |

## Every manager key in both views

The common TypeScript dispatcher is `src/manager.ts:404`; Rust's is `rust/src/manager.rs:434`.
Each non-exit transition re-renders the full grid. Arrow Up/Down are aliases of `k`/`j` and are
also fed by `screen.manager.all-views`.

| Key | Active view: TypeScript → Rust and effect | Archives view: TypeScript → Rust and effect | Evidence |
|---|---|---|---|
| `j` / Down | `handleActiveKey` `src/manager.ts:330` → `handle_active_key` `rust/src/manager.rs:478`; increment/clamp active selection, changing list highlight and detail. | `handleArchiveKey` `src/manager.ts:366` → `handle_archive_key` `rust/src/manager.rs:527`; increment/clamp archive selection and detail. | `active-j`, `active-arrow-down`, `archives-j`, `archives-arrow-down`. |
| `k` / Up | `src/manager.ts:328` → `rust/src/manager.rs:475`; decrement/saturate. | `src/manager.ts:364` → `rust/src/manager.rs:524`; decrement/saturate. | Corresponding `k` and arrow-up steps. |
| `y` | `copy` at `src/manager.ts:218` receives the selected annotation → `ManagerApp::copy` at `rust/src/manager.rs:547`; exact one-item Markdown is copied; success exits 0, an empty selection displays status. | Same helpers receive the selected archive's annotations in newest-first order (`src/manager.ts:368`, `rust/src/manager.rs:529`). | Success sessions `store.manager.active-y-success` / `archives-y-success` / `osc52-remote-copy`, and empty-actions. |
| `c` | `src/manager.ts:337` copies the displayed newest-first active list → `rust/src/manager.rs:492`; success exits. | Not handled by `src/manager.ts:346` or `rust/src/manager.rs:503`; clears any prior confirmation/status through normal dispatch, otherwise store/view unchanged. | `store.manager.active-c-success`, `osc52-remote-copy-all`, and `archives-c-ignored`. |
| `C` | `copyAndArchive` at `src/manager.ts:227` → `copyAndArchiveAnnotations` at `src/archive-workflow.ts:29`; Rust `rust/src/manager.rs:554` → `copy_and_archive_annotations` at `rust/src/archive_workflow.rs:27`. Order is load → copy → append archive → remove captured active IDs. Success exits; partial failure is reported without data loss. | Not handled; same no-op/confirmation-clear behavior as archive `c`. | Success and byte-diff in `store.manager.copy-archive`; `osc52-remote-copy-archive` proves the archive still runs when only the terminal copy landed; `archives-C-ignored`; workflow failure ordering has paired TS/Rust unit specs. |
| `d` | `deleteSelectedAnnotation` at `src/manager.ts:245` → `rust/src/manager.rs:575`; remove selected ID through locked atomic rewrite, reload, status `Annotation deleted.` | First press records the selected archive id and renders `Press d again…`; second matching press calls `deleteSelectedArchive` (`src/manager.ts:297`, `rust/src/manager.rs:639`) and atomically removes it. No selection displays `No archive selected.` Esc or another ordinary key cancels confirmation. | Active delete, archive confirm/cancel/double-confirm in all-views; empty-actions. |
| `D` | `src/manager.ts:321` / `rust/src/manager.rs:467`: first press renders `Press Shift+D again…`; second calls `clearActive` (`src/manager.ts:257`, `rust/src/manager.rs:589`), rewrites active JSONL empty, reloads, and displays `All active annotations cleared.` | Uppercase `D` is not an archive action; it cancels a pending archive confirmation like any non-`d` archive key, otherwise no store effect. | Active confirm/cancel/double-confirm and `archives-D-ignored` in all-views. |
| `r` | `reloadActive` (`src/manager.ts:55`, `rust/src/manager.rs:80`); success status `Reloaded.`, failure status is the store error. | `reloadArchives` (`src/manager.ts:66`, `rust/src/manager.rs:95`) with the same status rule. | Both reload steps in all-views; invalid/busy store process cases cover propagated store errors. |
| `u` | Not handled in active view; clears transient status/confirmation, leaves selection and stores unchanged. | `restoreSelectedArchive` at `src/manager.ts:271` → `restoreArchivedSet` at `src/archive-workflow.ts:71`; Rust `rust/src/manager.rs:606` → `restore_archived_set` at `rust/src/archive_workflow.rs:96`. Order is merge missing annotation IDs, then remove archive; partial removal failure keeps the archive and reports it. | `active-u-ignored`; archive restore in all-views; no-selection in empty-actions; paired workflow unit specs cover partial failures and concurrent active records. |
| Tab | `switchView` at `src/manager.ts:308` → `rust/src/manager.rs:651`; clear confirmation/status, switch view, reload destination store. | Same in reverse. | Both Tab directions in all-views and all archive exit sessions. |
| Esc | `src/manager.ts:406`: if confirming, clear confirmation/status and stay; otherwise cleanly exit. Rust `rust/src/manager.rs:440` is identical. | Same. | Active confirmation cancel and active exit; archive confirmation cancel and archive exit. |
| `q` | Common dispatcher exits 0 through cleanup. | Same. | Active `q` in all-views; `screen.manager.q-archives`. |
| Ctrl+C | Common dispatcher exits 0 through cleanup. | Same. | `screen.manager.control-c-active` and `control-c-archives`. |

## Rendered and exported products

| Product | TypeScript call path | Rust call path | Observable proof |
|---|---|---|---|
| Markdown export | `formatAnnotations` at `src/format.ts:44`; `fenceFor` at `:38`. | `format_annotations` at `rust/src/format.rs:67`; `fence_for` at `:52`. | Both emit `# Annotated context`, then newest-first `## Annotation N` sections. Optional source is `workspace_label / tab_label`; selected text and comment retain line breaks; selected text uses a backtick fence one longer than its longest run (minimum three); duplicate blank lines are collapsed; the document ends in exactly one `\n`. Populated/single copy and cross-read compare raw clipboard bytes using backticks, multiline text, and wide characters. |
| Terminal-safe text | `sanitizeTerminalText`/`wrapText` at `src/format.ts:5`/`:12`. | `sanitize_terminal_text`/`wrap_text` at `rust/src/format.rs:7`/`:26`. | Control characters are removed except newline/tab, tabs become four spaces, CRLF becomes LF, explicit newlines are preserved, and wrapping uses terminal cells. Editor and both manager views compare resulting cells. |
| Width and clipping | `charWidth`, `stringWidth`, `truncateToWidth` at `src/width.ts:37`/`:48`/`:55`; manager `clipped` at `src/manager.ts:77`. | `char_width`, `string_width`, `truncate_to_width` at `rust/src/width.rs:35`/`:47`/`:52`; manager `clipped` at `rust/src/manager.rs:681`. | Same zero-width controls/combining ranges and same wide ranges; truncation never splits a wide glyph and adds exactly one ellipsis cell. Wide input occurs in editor, list, detail, metadata, and archive snapshots. |
| Editor geometry | `render` at `src/editor.ts:77` and `layoutComment` at `src/layout.ts:9`. | `EditorApp::draw` at `rust/src/editor.rs:67` and `layout_comment` at `rust/src/layout.rs:14`. | At 86×22, identical selected-text cap/overflow marker, comment viewport, cursor cell, footer/status placement, and full clears. Every editor input has a post-step grid diff. |
| Manager geometry and labels | `render`/active/archive/footer at `src/manager.ts:198`/`:95`/`:138`/`:184`. | `ManagerApp::draw`/active/archive/footer at `rust/src/manager.rs:110`/`:162`/`:300`/`:409`. | At 98×28, identical 36%-clamped list, divider, selected marker/reverse cell region, detail width, newest-first labels, counts, metadata line, preview overflow, empty-state text, confirmation footer, help footer, and transient status. The archive clipping boundary that exposed the earlier one-cell defect is in every seeded archive screen. |

## Filesystem effects

| Observable | TypeScript call path | Rust call path | Exact product and evidence |
|---|---|---|---|
| State directory creation | `fs.mkdirSync(..., {recursive:true})` at `src/capture.ts:42` and `withStoreLock` at `src/store.ts:196`. | `create_dir_all` at `rust/src/cli.rs:84` and `create_private_dir_all` at `rust/src/store.rs:392`. | Process-default directory mode (0755 under harness umask 022), not forced 0700. `process.copy.empty` compares the directory mode. |
| Pending file | `src/capture.ts:44`/`:49`/`:52`. | `rust/src/cli.rs:85`/`:92` and `write_pending` at `:247`. | Name `pending-<epoch-ms>-<pid>.json`; mode 0600 on creation; bytes are one JSON object plus `\n`; property order `selectedText`, `context`, `capturedAt`. Capture cases byte/mode-diff it. |
| Pending consumption | `src/editor.ts:34` reads/parses, then `fs.rmSync(...,{force:true})` at `:39`. | `pending_from_env` at `rust/src/editor.rs:320`, then `remove_pending_file` at `:337`. | Delete only after successful read and semantic parse; missing-at-delete is ignored; other deletion errors fail startup. `screen.editor.pending-file-save` compares the consumed tree and saved JSONL; Rust regression `pending_removal_is_forceful_like_typescript` pins the delete race. |
| Handoff take | `handoffPath`/`takeHandoff` at `src/handoff.ts:17`/`:24`. | `handoff_path`/`take_handoff` at `rust/src/handoff.rs:12`/`:32`. | `$XDG_RUNTIME_DIR` else temp + `herdr-annotate-<uid>/selection`; missing/stat failure means absent; regular files ≤15 s old are decoded as UTF-8 with replacement; stale and blank values are rejected; every found node is removed; non-NotFound removal failure propagates. Context-skipped handoff remains. Capture handoff cases diff pending bytes and the runtime tree. |
| Active append | `appendAnnotation` at `src/store.ts:42`. | `append_annotation` / `append_annotation_context_first` at `rust/src/store.rs:77`/`:82`, sharing `append_annotation_record` at `:97`. | Append one compact JSON object plus `\n`; mode 0600 on creation. Capture-file editor order is `selectedText,capturedAt,context,id,comment,createdAt`; direct invocation fallback preserves TypeScript's distinct `selectedText,context,capturedAt,id,comment,createdAt`. `screen.editor.pending-file-save.state` and `screen.editor.edit-save.state` byte-diff both orders; two Rust regressions pin them. |
| JSONL read | `loadJsonLines` at `src/store.ts:150` and parsers at `src/types.ts:92`/`:103`. | `load_json_lines` at `rust/src/store.rs:227` and parsers at `rust/src/types.rs:133`/`:145`. | Missing file = empty; empty lines skipped; any malformed/nonconforming non-empty line rejects the whole store; unknown fields tolerated by parsers. Invalid-store and paired unit cases cover both stores. |
| Active rewrite | Remove/merge at `src/store.ts:54`/`:70` → `replaceJsonLines` at `:175`. | `remove_annotations_by_id`/`merge_annotations` at `rust/src/store.rs:120`/`:136` → `replace_json_lines` at `:253`. | Retained append order; merge by id without duplicates; canonical saved field order; temporary `.<annotations>-<pid>-<ms>.tmp`, mode 0600, one newline per record (zero bytes when empty), then rename; temp removed on failure. Manager state byte diffs prove final files and absence of leftovers. |
| Archive rewrite | Append/remove at `src/store.ts:99`/`:111` → `replaceJsonLines`. | `append_archived_set`/`remove_archived_set` at `rust/src/store.rs:173`/`:182` → `replace_json_lines`. | Entire archives store is atomically replaced. Outer order is `version,id,archivedAt,annotations`; inner annotations are canonical; trailing newline and 0600 mode match. Copy/archive, restore, and permanent-delete scripts byte-diff the products. |
| Lock acquire | `withStoreLock`/`acquireStoreLock`/`createStoreLock` at `src/store.ts:196`/`:217`/`:241`. | `with_store_lock`/`acquire_store_lock`/`create_store_lock` at `rust/src/store.rs:285`/`:296`/`:333`. | Per-store `.annotations.lock` or `.archives.lock`; directory mode 0700; `owner` mode 0600 with `<pid>:<uuid>\n`; exclusive directory creation. A fresh existing lock returns the exact busy error without mutation. |
| Lock steal | Staleness check/removal/retry at `src/store.ts:223`–`:238`, `isStaleLock` at `:261`. | Corresponding branch `rust/src/store.rs:313`–`:330`, `is_stale_lock` at `:345`. | Age ≥30 s is removed then acquired once; a competing recreation reports busy. `process.copy.busy-lock` and `stale-lock` compare exits, errors, and final lock trees. |
| Lock release | `releaseStoreLock` at `src/store.ts:269`, called in `finally` at `:210`. | `StoreLockLease::drop` at `rust/src/store.rs:52`. | Read current owner; remove recursively only if token still matches; ignore cleanup errors. Every state-compared completed store case asserts no owned lock remains; paired contention tests cover recovery. |

## Process effects

The editor pane argv is identical, including order:

```text
herdr plugin pane open --cwd <HERDR_PLUGIN_ROOT> --plugin annotate --entrypoint editor \
  --placement popup --width 88 --height 24 \
  --env HERDR_ANNOTATE_PENDING=<normalized-pending-path> --focus
```

It is constructed at `src/capture.ts:54` and `rust/src/cli.rs:97` and compared by every successful
capture case. The manager argv is constructed at `src/open-manager.ts:13` and `rust/src/cli.rs:216`:

```text
herdr plugin pane open --cwd <HERDR_PLUGIN_ROOT> --plugin annotate --entrypoint manager \
  --placement popup --width 100 --height 30 --focus
```

`runHerdr` (`src/herdr.ts:11`) and `run_herdr` (`rust/src/herdr.rs:22`) ignore stdin/stdout, capture
stderr, and use `HERDR_BIN_PATH` or `herdr`. Nonzero child status returns trimmed child stderr; if it
is empty or spawning fails, the exact fallback is `herdr <space-joined argv> failed`. Manage success,
stderr failure, and empty-stderr failure are differential cases.

Notifications are best effort and never alter the primary exit. Both sides call:

```text
herdr notification show <title> [--body <body>]
```

The compared title/body pairs are `Nothing to annotate`, `Annotate failed`, `No annotations`,
`Annotations copied`, `Copy failed`, `Annotations copied and archived`, `Copy and archive failed`,
`Copy and archive incomplete`, and `Unable to open annotations`; their bodies are listed in the
error/reporting table below or generated from the exact annotation count.

Clipboard candidates and arguments are defined at `src/clipboard.ts:13`/`:30` and
`rust/src/clipboard.rs:12`/`:46`:

| Platform | Read order | Write order |
|---|---|---|
| macOS | `pbpaste` | `pbcopy` |
| Windows | `powershell.exe -NoProfile -NonInteractive -Command "Get-Clipboard -Raw"` | `powershell.exe -NoProfile -NonInteractive -Command "$input | Set-Clipboard"` |
| Linux/other Unix | `wl-paste --no-newline`; `xclip -selection clipboard -out`; `xsel --clipboard --output` | `wl-copy`; `xclip -selection clipboard -in`; `xsel --clipboard --input` |

### Pane copies and OSC 52

Herdr 0.9.0 forwards OSC 52 sequences emitted by pane output to the viewing client, so a copy made
inside a pane can reach the clipboard of the machine the person is sitting at even when the plugin
runs on a different server. Only the manager pane can use this path: the global `copy-context` and
`copy-archive` actions run with piped stdout and no terminal, so their behavior is unchanged.

| Behavior | TypeScript | Rust | Proof |
|---|---|---|---|
| Sequence encoding | `osc52ClipboardSequence` at `src/pane-clipboard.ts:18` builds `ESC ] 52 ; c ; <base64 of the UTF-8 bytes> BEL`. | `osc52_clipboard_sequence` at `rust/src/pane_clipboard.rs:42` over the same bytes, with a hand-rolled base64 at `:17`. | `test/pane-clipboard.test.ts` and `pane_clipboard::tests` assert the exact bytes for `hi` (`\x1b]52;c;aGk=\x07`), an empty string, and multi-byte UTF-8; `store.manager.*` compares the sequences observed on real PTYs. |
| Emission point | `paneClipboardWriter` at `src/pane-clipboard.ts:49` wraps `writeClipboard`; `src/manager.ts:41` injects it into both manager copy call sites. | `write_pane_clipboard` at `rust/src/pane_clipboard.rs:68` wrapped by `pane_clipboard_write` at `rust/src/manager.rs:703` and injected at `:548`/`:558`. | Both runtimes emit exactly once per writer call, after the native attempt and before the next frame, so the emitted sequence lists match step for step. |
| Native failure | Either destination landing is a successful copy (`src/pane-clipboard.ts:56`). A remote server commonly has no clipboard tool at all, and the sequence is the copy that actually reached the person, so it is not reported as a failure and does not stop a copy-and-archive before its archive step. | Identical rule at `rust/src/pane_clipboard.rs:75`. | The three `store.manager.osc52-remote-*` cases run with the native writer failing: the copy exits 0, `C` still writes its archive and clears the active list, and both runtimes emit the same sequence. |
| Neither destination | Only a copy that reached neither the clipboard nor the terminal fails, carrying the unchanged native error. | Same. | Paired unit specs; unreachable from a pane whose stdout is a live terminal. |
| Oversized payloads | `exceedsCommonOsc52Limit` at `src/pane-clipboard.ts:26`. | `exceeds_common_osc52_limit` at `rust/src/pane_clipboard.rs:47`. | Terminals commonly refuse a base64 payload over 74994 bytes. Both runtimes emit the full sequence anyway and never truncate a copy; the predicate exists so the limit is stated rather than silently applied. |

Replacing one runtime's emitter with a no-op and rerunning the harness fails every manager copy
case, so these comparisons are not vacuously green.

Because a pane's stdout is a live terminal, a manager copy no longer fails there. `No supported
clipboard writer is available` therefore remains reachable from the global actions, which have no
terminal, but not from the manager; the manager's copy-failure frames were removed from
`screen.manager.all-views` for that reason.

Readers return the first exit-0 stdout, decoded with UTF-8 replacement. Writers pipe the exact UTF-8
Markdown to stdin and accept the first exit-0 adapter. Child stdout/stderr is suppressed. The fake
adapters log every attempt and capture writer stdin; macOS and Ubuntu CI together prove both Unix
branches. Windows arguments are source-mapped and build-checked, not run by this harness.

### Exit codes

| Path | Exit |
|---|---|
| Successful capture, empty capture after notification, successful/empty copy, successful/empty copy-archive, successful manage pane open | 0 |
| Missing required env, no clipboard adapter, store parse/lock/access failure, or pane-open failure | 1 after one stderr line; action commands also attempt their failure notification |
| Editor/manager initialization error | 1 with stderr, no action-level notification |
| Editor Esc/Ctrl+C, manager Esc/q/Ctrl+C, editor SIGTERM, manager SIGHUP | 0 after terminal restoration |
| Successful editor save | 0 after final `Saved.` frame and 250 ms delay |
| Manager copy/copy+archive success | 0 after clipboard/store completion; a pane copy succeeds whenever the clipboard write or the OSC 52 emission lands |

## Error and failure strings

These are all Lite-authored error strings and templates. A `<CODE>` is the operating-system code
projected by `safeFileError` (`src/store.ts:285`, `rust/src/store.rs:396`); omitting the code leaves
the prefix exactly as shown.

| Exact string or template | TypeScript emitter | Rust emitter | Proof |
|---|---|---|---|
| `HERDR_PLUGIN_STATE_DIR is not set` | `src/capture.ts:22`, `src/export.ts:9`, `src/export-archive.ts:48`, `src/manager.ts:33` | `rust/src/cli.rs:69`/`:125`/`:185`, `rust/src/manager.rs:725` | Missing-state process cases. |
| `HERDR_PLUGIN_ROOT is not set` | `src/capture.ts:24`, `src/open-manager.ts:7` | `rust/src/cli.rs:70`/`:215` | Missing-root cases. |
| `No supported clipboard reader is available` | `src/clipboard.ts:59` | `rust/src/clipboard.rs:106` | `process.capture.no-clipboard`. |
| `Missing pending annotation` | `src/editor.ts:32` | `rust/src/editor.rs:326` | `process.editor.missing-pending`. |
| `Pending annotation is invalid` | `src/editor.ts:37` | `rust/src/editor.rs:332` | `process.editor.invalid-pending`. |
| `Write a comment before saving.` | `src/editor.ts:125` | `rust/src/editor.rs:256` | Editor empty-save frame. |
| `Plugin state directory is unavailable.` | `src/editor.ts:131` | `rust/src/editor.rs:260` | Editor missing-state frame. |
| `Unable to save annotation[ (<CODE>)]` | `src/store.ts:48` | `rust/src/store.rs:100` | Store error catalog and paired store/editor tests. |
| `Unable to read annotations (invalid data)` / `Unable to read archives (invalid data)` | `src/store.ts:163`/`:166` | `rust/src/store.rs:245`/`:247` | Invalid active process case and paired active/archive tests. |
| `Unable to read annotations (<CODE>)` / `Unable to read archives (<CODE>)` | `src/store.ts:171` | `rust/src/store.rs:235`/`:240` | Source/error catalog; OS-specific access cases remain unit-level. |
| `Unable to access annotations[ (<CODE>)]` / `Unable to access archives[ (<CODE>)]` | `src/store.ts:204` | `rust/src/store.rs:290` | Static catalog; normal missing-dir creation is differential. |
| `Unable to lock annotations[ (<CODE>)]` / `Unable to lock archives[ (<CODE>)]` | `src/store.ts:230`/`:257` | `rust/src/store.rs:306`/`:326` | Static catalog and contention tests. |
| `Annotations are busy; try again.` / `Archives are busy; try again.` | `src/store.ts:224`/`:235` | `rust/src/store.rs:314`/`:324` | Active busy-lock differential plus paired per-store tests. |
| `Unable to update annotations[ (<CODE>)]` / `Unable to update archives[ (<CODE>)]` | prefixes supplied at `src/store.ts:135`/`:147` | `rust/src/store.rs:202`/`:223` | Static catalog and rewrite failure unit paths. |
| `Nothing to copy.` | `src/manager-copy.ts:18` | `rust/src/manager_copy.rs:20` | `screen.manager.empty-actions`. |
| `Nothing to copy and archive.` | `src/archive-workflow.ts:35` | `rust/src/archive_workflow.rs:44` | `screen.manager.empty-actions`. |
| `No archive selected.` | `src/manager.ts:274`/`:351` | `rust/src/manager.rs:505`/`:609` | `screen.manager.empty-actions`. |
| `Copied and archived, but active annotations remain: <store error>` | `src/manager.ts:239`, `src/export-archive.ts:35` | `rust/src/manager.rs:570`, `rust/src/cli.rs:167` | Paired workflow partial-failure tests plus static catalog. |
| `Annotations restored, but the archive remains: <store error>` | `src/manager.ts:289` | `rust/src/manager.rs:628` | Paired workflow partial-failure tests plus static catalog. |
| Child `herdr` stderr, or `herdr <argv> failed` | `src/herdr.ts:20` | `rust/src/herdr.rs:32`–`:39` | Manage child-stderr and empty-stderr cases; capture pane failure. |

`Unable to save annotation.` (with a period) at `src/editor.ts:146` is specifically a Bun dynamic
module-loader failure. There is no corresponding runtime condition in the statically linked binary;
actual store-open/write failures take the mapped `Unable to save annotation[ (<CODE>)]` path on both
sides. JSON-parser and raw filesystem diagnostics emitted before a Lite-defined wrapper are supplied
by Bun or the Rust standard library, not authored stable strings; both processes fail and surface the
native diagnostic, but those platform/runtime wordings are not claimed as a portable Lite contract.

## Test and harness coverage

The existing behavior-spec mapping remains in `docs/rust-lite-parity.md`. The Rust unit modules mirror
all TypeScript spec files: `types`, `paths`, `handoff`, `format`, `width`, `layout`, `store`,
`archive-workflow`, and `manager-copy`. Rust additionally has `TestBackend` editor/manager frames and
subprocess command tests. The differential harness is independent of those expected-value tests: its
oracle is the current TypeScript implementation running on the same fixture.

Notable differential groups:

- `process.capture.*`: context > handoff > clipboard precedence; stale/blank/lossy-UTF-8 handoff;
  invalid context; empty; missing adapter/env; pane success/failure and pending cleanup.
- `process.copy.*`: missing/empty/single/plural/invalid stores; writer failure; fresh/stale locks.
- `process.copy-archive.*`: missing/empty stores; a complete archive byte-compared afterwards; writer
  failure leaving both stores untouched.
- `manifest.*`: both Lite manifests declare the same action and pane ids, titles, descriptions,
  contexts, placements, geometry, platform gates, and per-runtime argv, and the harness drives every
  declared entrypoint.
- `process.manage.*`, `process.editor.*`, `process.manager.*`: argv/error fallback and initialization.
- `screen.editor.*`: both pending sources, every requested edit key, save branches, cancel keys, and SIGTERM.
- `screen.manager.*`: both views, every view-valid key, ignored cross-view keys by handler mapping,
  both confirmation flows, empty actions, exit keys, and SIGHUP.
- `store.manager.*`: successful clipboard-only and copy/archive products; the three
  `store.manager.osc52-remote-*` cases copy with the native writer failing, which is the
  remote-server shape of issue #40; `store.cross-read` proves each implementation parses and exports
  the other's editor-written record.

The harness itself asserts the requested key-coverage set before it can print green.

## Findings fixed while building the proof

Each observed mismatch was fixed in a separate commit whose message cites this map section:

- `b50f388`: missing store directories now use the process-default mode, matching Bun.
- `28e3131`: direct invocation fallback preserves its distinct TypeScript JSON property order.
- `1d75160`: handoff UTF-8 decoding uses replacement characters, matching Bun.
- `89e41d8`: SIGTERM/SIGHUP exits restore the terminal and return 0.
- `53ef811`: pending deletion ignores a missing-at-delete race like `{ force: true }`.
- `3cb49ec`: non-NotFound handoff deletion failures now propagate instead of silently falling back.

No TypeScript source changed.

## Deliberate divergences

There is one. TypeScript delegates manager timestamps to `Date.prototype.toLocaleString()` and the
host's locale database (`src/manager.ts:134`, `:160`, `:171`). Rust parses into local time and emits
the en-US shape explicitly (`rust/src/manager.rs:691`). The persisted ISO timestamp, ordering, export,
and en-US display are identical. A non-en-US host can display localized punctuation/order in
TypeScript while Rust stays en-US. The harness pins UTC and en-US and compares the literal seeded
timestamp cells; it does not normalize them.

Windows remains unexecuted by this Unix PTY harness, as required. It is a verification gap for the
separate promotion track, not an intentional product behavior difference.
