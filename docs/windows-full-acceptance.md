# Windows Full manifest acceptance checklist

Decision: add `windows-full/herdr-plugin.toml`. Keep the root manifest unchanged.
The new variant requires Herdr 0.9.0 and starts the native TUI directly. No
launcher process may sit between Herdr and the TUI. `q` quits the TUI; Escape is
not a quit binding.

Prepared 2026-09-07 against plugin commit
`46cbf5dab1746dfeb46eb160c33990b0e0de15b0`, with TUI pin `0.6.0`.
This checklist supersedes the open Windows flip design in onboarding section 7
and the older single-manifest proposal in the Windows spec. It records acceptance
requirements, not completed verification. Every box starts unchecked.

## 1. Manifest and compatibility

- [ ] Add a GitHub-installable `windows-full/` subdirectory with plugin id
  `annotate`, name `Annotate`, the root plugin version, `platforms = ["windows"]`,
  and `min_herdr_version = "0.9.0"`. Do not rewrite the manifest during build.
- [ ] Leave `herdr-plugin.toml` and `lite/herdr-plugin.toml` unchanged, including
  their `0.8.0` minimums. Preserve
  the root Unix wrapper and its platform gates. Do not change the TUI development
  manifest as part of this addition.
- [ ] Compare parsed action ids, titles, descriptions, and contexts with the root
  manifest. Require exact set equality, including `copy-archive`; do not accept a
  test that checks only a subset. The current ids, titles, and contexts are:

  | Id | Title | Contexts |
  |---|---|---|
  | `capture` | Annotate selection | `pane` |
  | `copy-context` | Copy annotations as context | `global` |
  | `copy-archive` | Copy annotations as context and archive them | `global` |
  | `manage` | Manage annotations | `global` |
  | `open` | Annotate: open here | `workspace`, `pane` |
  | `open-link` | Annotate this file | `pane` |
  | `last` | Annotate: agent's last message | `pane` |

- [ ] Require exactly three panes: `editor` (Annotate, popup, 88×24), `manager`
  (Annotations, popup, 100×30), and one `doc` (Annotate, overlay). The `doc`
  command must be exactly `["./bin/plannotator-tui.exe", "herdr", "pane"]`.
  No shell, PowerShell, command-string interpolation, or fallback launcher may sit
  between Herdr's pane process and the TUI.
- [ ] Keep `open` and `open-link` as direct argv ending in `herdr open`, and
  `last` as direct argv ending in `herdr last`, all using
  `./bin/plannotator-tui.exe`. Preserve the `markdown-file` link handler's title,
  pattern, and `open-link` action. All actions, panes, the handler, and the build
  must be effective on Windows; no inherited Unix gate may disable them.
- [ ] Reuse the shared native runtime with paths valid from `windows-full/`
  (as `lite/` does with `../bin/`). Verify those paths from an installed checkout,
  not just the repository root. Preserve shared annotation state and archive
  formats; changing variants must not migrate or clear them.
- [ ] On isolated installs, Herdr 0.8.2 rejects the new variant with
  `plugin_requires_newer_herdr`; 0.9.0 accepts it. Root and Lite remain installable
  on 0.8.2. Existing pinned installs receive no change until explicitly
  reinstalled. Record the actual base version and commit for any preview/master
  build tested; do not infer its behavior from the channel name.
- [ ] Extend manifest checks and CI path filters to include `windows-full/**`.
  Preserve existing root/Lite/development-manifest assertions rather than
  changing them to expect Windows Full everywhere. Run onboarding section 8's
  applicable static, unit, manifest, and Lite regression checks with zero
  unexpected failures, plus Unix fetcher regression checks.

## 2. PowerShell fetch contract

- [ ] Use one Windows build entry invoking `powershell.exe` with
  `-NoProfile -NonInteractive -ExecutionPolicy Bypass -File` and a resolvable
  fetcher path. Exercise that exact entry on Windows, including Windows
  PowerShell rather than testing only under `pwsh`.
- [ ] Read the repository's `plannotator-tui.version` as the single release pin;
  stage the binary and stamp at `windows-full/bin/plannotator-tui.exe` and
  `windows-full/bin/plannotator-tui.version`. The current shared fetcher derives
  its destination from its own script directory and would write root `bin/` if
  invoked unchanged. Require an explicit, tested solution for the subdirectory.
  Builds must not depend on `HERDR_PLUGIN_ROOT` being supplied by Herdr, a
  particular caller cwd, or an absolute temporary checkout path surviving install.
- [ ] Select the OS architecture: X64 downloads
  `plannotator-tui-x86_64-pc-windows-msvc.exe`; Arm64 downloads
  `plannotator-tui-aarch64-pc-windows-msvc.exe`. Verify both assets and
  `SHA256SUMS` exist for the pinned release. Never silently substitute x64 on
  ARM64. A pin bump requires its own release verification.
- [ ] Download from
  `https://github.com/plannotator/plannotator-tui/releases/download/v<PIN>`.
  Require exactly one checksum entry for the exact asset filename and compare
  SHA-256 before promoting the binary. Test a valid checksum, mismatch, missing
  entry, duplicate entry, and network failure. Reject malformed checksum data.
- [ ] Preserve the existing failure contract: remote download, checksum,
  unsupported-architecture, and replacement failures warn and exit zero so Lite
  remains available. A failed fresh fetch must not leave an apparently installed
  binary/stamp; a failed update must preserve prior binary/stamp bytes. CI must
  assert these outcomes, not treat exit zero as proof Full was installed.
- [ ] Test replacement of an existing binary, a locked executable, and failure
  while writing the stamp. Verify rollback of both files, normal temporary-file
  cleanup, and retained recovery files with a diagnostic if rollback itself
  fails. Preserve the shared fetcher's existing default behavior for other callers.
- [ ] Preserve `PLANNOTATOR_TUI_BIN`: an explicit local file overrides an existing
  matching stamp, copies exact bytes, and is stamped with the pin. An empty or
  invalid override and an empty pin fail nonzero. Label override tests as local
  builds, not release-checksum proof. With no override, a matching binary/stamp
  is an idempotent no-download success. Treat `PLANNOTATOR_TUI_RELEASE_BASE` as a
  loopback-test seam, not production-download evidence.

## 3. Required GitHub Windows runner proof

These are merge requirements for the **CI-supported preview**. A linked manifest,
mocked subprocess, ordinary redirected console, cross-compile, or skipped test
does not establish that a real Herdr pane works.

- [ ] Pin and verify the official Herdr **0.9.0 Windows x64** archive and record
  its SHA-256 and `--version`. Use that executable for the server and every CLI
  call. Record the runner image/Windows build, architecture, PowerShell version,
  plugin SHA/subdirectory, TUI pin, downloaded asset hash, and native binary version.
- [ ] Test the reviewed checkout through the subdirectory install/build lifecycle
  in an isolated Herdr config/state directory. Also prove the GitHub subdirectory
  install syntax against the reviewed SHA when it is remotely available. Local
  `plugin link` alone is not evidence of GitHub fetching, building, and relocating
  the checkout. Retain the resulting installed path and parsed manifest.
- [ ] Perform a clean production fetch with both override variables absent.
  Assert the final installed executable's checksum, stamp, version, and location
  under the installed `windows-full/`. Use that real release binary in the pane.
- [ ] Start a disposable named Herdr session with an active client backed by a
  real Windows ConPTY. Open `doc` in a review directory outside the checkout,
  using the installed entry. Capture enough terminal output to prove a unique
  fixture document rendered; a pane id or live PID alone is insufficient.
- [ ] Prove native argv, cwd, and environment at the pane boundary. Check
  `HERDR_PLUGIN_ROOT` identifies the installed subdirectory,
  `HERDR_PLUGIN_ID=annotate`, the isolated `HERDR_PLUGIN_STATE_DIR`, `HERDR_ENV`,
  `HERDR_PANE_ID`, and valid `HERDR_PLUGIN_CONTEXT_JSON` with the originating
  pane/folder. Check `HERDR_BIN_PATH` when supplied. Verify the TUI pane id and
  originating pane id are not confused. A separate native diagnostic probe may
  establish these values; it cannot replace the real-TUI rendering test.
- [ ] Pass a relative `PLANNOTATOR_TUI_FILE` that exists only in the review cwd,
  then an absolute path with spaces and Unicode. Assert each intended document
  renders. Verify delivery target variables supplied by the launcher survive
  unchanged. Exercise `open`, `open-link` (including a percent-encoded `file://`
  path), and `last` with controlled context/transcript fixtures. Record fixture
  evidence separately from real-agent integration.
- [ ] Open Full with inert review-folder `.env` fixtures that would change a
  launcher process. Full must render the intended document without loading those
  files. Smoke the reused Lite action and pane paths from the installed
  subdirectory.
- [ ] Send `q` through ConPTY input to the normal viewer. Assert TUI exit code
  zero, closure of the review pane, and a responsive original pane/client. Repeat
  in the default overlay and supported popup/split placements. Record Escape
  behavior separately as an upstream Herdr observation; do not require it to quit.
- [ ] In separate runs, close the pane through Herdr, terminate its TUI process,
  and shut down the disposable session with a review open. Record observed exit
  status and bound the wait. Track PID, creation time, executable path, and
  descendants before/after; do not rely only on parent PID after termination.
  Require no surviving test TUI or child processes, no hung ConPTY teardown, and
  no locked staged executable. Teardown must also run after assertions fail.
- [ ] Include a controlled TUI failure and capture its actual nonzero process
  status. Distinguish normal `q` exit from forced termination, which need not
  report a Unix signal number on Windows.
- [ ] Run the path matrix below with distinct plugin roots and review folders.
  Record actual strings and lengths received by the child. Verify `.exe` stays
  literal, arguments stay separate, and no shell metacharacters expand. Check
  Herdr's normal and extended-length representations refer to the same directory;
  do not blindly remove `\\?\` or assume 0.9.0 shortens every path.

  | Path case | Required evidence |
  |---|---|
  | Spaces, Unicode, apostrophe, `&`, parentheses, `$`, `%`, brackets | Fetch, install/link, render, `q`, teardown |
  | Ordinary drive path and equivalent `\\?\C:\...` root | Correct root/cwd/argv and real pane lifecycle |
  | Root near the legacy path limit; root plus `bin/plannotator-tui.exe` beyond it; long review-file path | Measured lengths, fetch/staging outcome and real pane lifecycle |
  | UNC and `\\?\UNC\server\share\...` | Use an isolated runner share where available; otherwise mark UNVERIFIED with the setup limitation |

  A failed or unavailable required local-drive case blocks a claim that it
  passed. Any platform limitation must be reported for review, not silently
  skipped or fixed by changing the root manifest. An unavailable UNC case remains
  an explicit preview limitation pending native-host evidence.

- [ ] Test root/Lite/Windows Full replacement in isolated installs: exactly one
  `annotate` registration, selected subdirectory in effect, preseeded annotations
  and archives byte-identical after switching. Retain CI logs, sanitized context,
  terminal transcript, exit statuses, process snapshots, and cleanup results as
  artifacts, including on failure. Never use the human's session or installed state.

## 4. Human native-Windows qualification

Keep the following rows **UNVERIFIED** in the review and release notes until a
human records them on native Windows. Automated runner evidence remains valid
for its specific assertions; it does not qualify the complete user experience.

- [ ] Windows Terminal and another ConPTY-capable terminal: focus and restoration,
  resize, overlay/popup/split, keyboard and mouse selection, paste, Unicode/wide
  text, and `q`. Observe Escape without advertising it as a TUI quit key.
- [ ] Real supported agents: select the older of two sessions in the same cwd,
  verify exact-session precedence, and send feedback to the invoking pane.
  Exercise a blocked/closed target and clipboard fallback. Require evidence per
  agent/version; fixtures do not qualify agents that were not run.
- [ ] Standalone copy and Herdr fallback reach the outer Windows clipboard,
  including OSC 52 behavior in both terminals. Runner output containing an
  escape sequence does not prove clipboard delivery.
- [ ] Native ARM64 installation, executable architecture, rendering, shutdown,
  and agent round trip. Asset existence, checksum verification, cross-compilation,
  and x64 emulation do not qualify native ARM64 execution.
- [ ] Any untested UNC/network-share or long-path configuration from the CI
  matrix. Record Windows build, architecture, terminal/shell, Herdr version/ref,
  plugin SHA/subdirectory, TUI version, agent versions, and PASS/FAIL/UNVERIFIED
  for each human-tested row. Retain **CI-supported preview** until this matrix
  supports a stronger claim.

## 5. Install command and required README wording

- [ ] After the subdirectory is available, test this public command verbatim:

  ```text
  herdr plugin install plannotator/herdr-annotate/windows-full
  ```

  For review CI, add `--ref <FULL_REVIEWED_COMMIT_SHA> --yes`, replacing the
  placeholder with the exact remotely available commit. Do not substitute a
  GitHub tree URL or an unsupported `--subdir` flag.

- [ ] Add the following installation wording once the required CI passes. Adjust
  the surrounding requirements/install table so it does not imply that the root
  command enables Windows Full or that Windows Full supports Herdr 0.8.x:

  > **Windows Full (CI-supported preview)** requires Herdr 0.9.0 or later and
  > Windows PowerShell. Install it with:

  ```text
  herdr plugin install plannotator/herdr-annotate/windows-full
  ```

  > The installer downloads the pinned native Windows TUI and verifies its
  > SHA-256 checksum. Document and agent-reply review starts that executable
  > directly. The Lite annotation tools run the native `herdr-annotate` binary the
  > same install downloads.
  > Press `q` to close the TUI. Escape is not a TUI quit key.
  >
  > Native terminal interaction, real-agent delivery, outer clipboard behavior,
  > and native ARM64 execution remain **UNVERIFIED** until the human Windows
  > qualification matrix is recorded. See the Windows acceptance results for
  > tested versions and path limitations.
  >
  > If the download or checksum check warns that Full review is unavailable,
  > Lite tools remain available. Rerun the same install command to retry.
  > Herdr pins plugin installs to a commit and does not update them automatically;
  > rerun the install command to update.
  >
  > All Annotate variants use the same plugin id. Installing another variant
  > replaces the selected variant and preserves saved annotations and archives.
  > For Windows on Herdr 0.8.x, install Lite:

  ```text
  herdr plugin install plannotator/herdr-annotate/lite
  ```

  > For Full on macOS or Linux, keep using:

  ```text
  herdr plugin install plannotator/herdr-annotate
  ```

  The acceptance-results link must resolve to a published report in the repo or
  PR, not this machine's absolute path. List any additional UNVERIFIED path rows
  there. Do not label the preview fully supported based only on green CI.

## 6. Reviewer sign-off

Answer onboarding section 9's four questions with links to the reviewed diff,
CI run/artifacts, and any human results:

1. Which installed commits and Herdr versions execute the new path? Distinguish
   existing pinned root/Lite installs from explicit Windows Full installs;
   include the 0.8.2 rejection and the versions actually tested.
2. Which invariants are touched, and which checks preserve manifest parity,
   the Unix wrapper, Lite behavior, state formats, and checksum-verified assets?
3. What ran live, on which OS/architecture and Herdr version? Separate native
   runner ConPTY evidence, fixtures, human sessions, and source-only inspection.
4. What remains UNVERIFIED, what evidence would close each row, and does any
   missing required CI proof block acceptance of the preview?
