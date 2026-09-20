![Herdr Annotate](assets/herdr-annotate.webp)

# herdr-annotate
Annotate inside [Herdr](https://github.com/herdrdev/herdr): comment on any terminal text, review whole Markdown documents and your agent's replies, and send the feedback straight back to the agent. Document review is powered by [plannotator-tui](https://github.com/plannotator/plannotator-tui), which also runs on its own outside Herdr.

<p align="center">
  <a href="https://github.com/backnotprop/plannotator">
    <img src="./assets/star-plannotator.svg" width="280" alt="Like this? Star Plannotator">
  </a>
</p>

**Watch the demos**

[![Demo: Full install](https://img.shields.io/badge/%E2%96%B6%20Demo%3A%20Full%20install-f7ca5e?style=flat-square&labelColor=171429)](https://x.com/plannotator/status/2093419561077154287)
[![Demo: Lite install](https://img.shields.io/badge/%E2%96%B6%20Demo%3A%20Lite%20install-c9c6f1?style=flat-square&labelColor=171429)](https://x.com/plannotator/status/2092757422322627008)

## Requirements

- Herdr 0.9.0 with client clipboard support (or compatible patched build)
- macOS, Linux, or Windows

There is no runtime to install. Both installs download a small prebuilt `herdr-annotate` binary and verify its SHA-256 checksum.

Global copy actions (`copy-context`, `copy-archive`) forward directly to the connected viewing client via Herdr's clipboard API. For the manager popup's local copy fallback, install `wl-clipboard`, `xclip`, or `xsel` on Linux when running on a local desktop session alongside terminal OSC 52.

On Windows, native Herdr plugin support is preview/best-effort. Local clipboard fallback uses PowerShell; no extra clipboard package is required. The install, keybinding, configuration check, reload, and use instructions below also apply on Windows.

## Install

Pick one. Installing the other later just swaps it (same plugin id). An install stays on the commit it came from; run the same command again to move to the current release.

<img src="assets/install-full.svg" width="200" align="left" alt="Full">

**Full:** annotate terminal text, review documents and agent replies, send feedback to the agent.
Wraps [Plannotator TUI](https://github.com/plannotator/plannotator-tui) (macOS and Linux today). [Demo](https://x.com/plannotator/status/2093419561077154287)

```sh
herdr plugin install plannotator/herdr-annotate
```

<br clear="all">

<img src="assets/install-lite.svg" width="200" align="left" alt="Lite">

**Lite:** the simple version: select text, `prefix+a`, comment in a popover. [Demo](https://x.com/plannotator/status/2092757422322627008)

```sh
herdr plugin install plannotator/herdr-annotate/lite
```

<br clear="all">

> **Required.** Bind the keys in Herdr's config.

<details open>
<summary><b>Full install keys:</b> terminal annotations + document and agent-reply review</summary>

```toml
# Terminal annotations
[[keys.command]]
key = "prefix+a"
type = "plugin_action"
command = "annotate.capture"
description = "annotate text"

[[keys.command]]
key = "prefix+shift+a"
type = "plugin_action"
command = "annotate.copy-context"
description = "copy annotations as context"

[[keys.command]]
key = "prefix+ctrl+a"
type = "plugin_action"
command = "annotate.copy-archive"
description = "copy annotations as context and archive them"

[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "annotate.manage"
description = "manage annotations"

# Document review (plannotator-tui)
[[keys.command]]
key = "prefix+o"
type = "plugin_action"
command = "annotate.open"
description = "review documents in this folder"

[[keys.command]]
key = "prefix+shift+o"
type = "plugin_action"
command = "annotate.last"
description = "review the agent's last reply"
```

</details>

<details>
<summary><b>Lite install keys:</b> terminal annotations only</summary>

```toml
[[keys.command]]
key = "prefix+a"
type = "plugin_action"
command = "annotate.capture"
description = "annotate text"

[[keys.command]]
key = "prefix+shift+a"
type = "plugin_action"
command = "annotate.copy-context"
description = "copy annotations as context"

[[keys.command]]
key = "prefix+ctrl+a"
type = "plugin_action"
command = "annotate.copy-archive"
description = "copy annotations as context and archive them"

[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "annotate.manage"
description = "manage annotations"
```

</details>

Check and reload:

```sh
herdr config check
herdr server reload-config
```

## Use

### Annotate terminal text

| Key | Action |
|---|---|
| `Ctrl+B A` | comment on the selected text · `Ctrl+S` saves |
| `Ctrl+B Shift+A` | copy all annotations as Markdown |
| `Ctrl+B Ctrl+A` | copy all annotations as Markdown, then archive them |
| `Ctrl+B M` | manage · `y` copy one · `c` copy all · `Shift+C` copy and archive · `Tab` archives (`y` copy · `u` restore · `d d` delete) |

Global copy actions (`Ctrl+B Shift+A` and `Ctrl+B Ctrl+A`) forward annotations directly to the viewing client's clipboard via Herdr's clipboard API. Copies made inside the manager pane also emit OSC 52, reaching the viewing client on local or remote sessions.

### Review documents and agent replies

Full install. Works with Claude Code, Codex, pi, Copilot CLI, Droid, Oh My Pi, Hermes CLI and OpenCode (1 and 2).

| Key | Opens |
|---|---|
| `Ctrl+B O` | this folder, with a file tree |
| `Ctrl+B Shift+O` | the agent's recent replies |
| Ctrl-click a `file://…md` link | that file |

**Send** (or `E`) makes the review the agent's next message. `q` closes.

| Option | Where |
|---|---|
| Agents request reviews themselves | `npx skills add plannotator/herdr-annotate --skill plannotator-tui -g` |
| Open as full tab, split, or popup | `[herdr] placement = "overlay" \| "split" \| "popup"` in `~/.config/plannotator-tui/config.toml` |
| Use without Herdr | [plannotator-tui](https://github.com/plannotator/plannotator-tui) |

### Remote sessions

Over SSH or saved machine federation, terminal annotations work directly from the viewing client:
- Mouse selections stay highlighted across mouse-up (with automatic copying preserved) and across prefix keys (`prefix+a`), capturing the remote selection cleanly into the annotation dialog.
- Global copies (`prefix+shift+a` and `prefix+ctrl+a`) route through Herdr's clipboard API, delivering formatted Markdown directly to your viewing client.
- For standalone `herdr --remote <host>` connections, connect with `--remote-keybindings server` (or configure remote machine profiles via `herdr machine add`) so the remote server's plugin actions are published to your client.

## Selection limits

Herdr Annotate receives the active terminal selection directly from Herdr's client shell invocation context. When triggered without an active Herdr terminal selection, it falls back to handoff text or clipboard reads. The plugin cannot read selection state from Neovim or another internal terminal application; use the Neovim mapping below to hand selections over directly.

## Development

The plugin is a Rust binary in `rust/`.

```sh
cargo test --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
bash scripts/stage-local.sh    # build and stage bin/herdr-annotate.exe
herdr plugin link "$PWD"       # or "$PWD/lite" for the Lite variant
```

`herdr plugin link` does not run manifest build hooks, so stage the binary first. It also replaces
any existing `annotate` link; link the other directory to switch back.

`bash scripts/lite-regression.sh` checks the runtime against goldens recorded from the retired Bun
runtime. [docs/lite-testing.md](docs/lite-testing.md) covers what it compares and everything else
that guards Lite.

To test a local plannotator-tui build instead of the pinned release, put it in `bin/`
before linking: `PLANNOTATOR_TUI_BIN=/path/to/plannotator-tui bash scripts/fetch-plannotator-tui.sh`.

Before a release, `HERDR_SESSION=<disposable session> bash scripts/smoke.sh` installs fresh, upgrades
from the first shipped commit, installs lite, swaps to full, and opens the manager and review panes,
then restores whatever was installed.

## Neovim integration

Add this visual-mode mapping to `~/.config/nvim/lua/config/keymaps.lua` for LazyVim, or to `init.lua`:

```lua
vim.keymap.set("x", "<leader>a", function()
  -- Hand the selection to the plugin through a file: works on headless servers too.
  vim.cmd('normal! "zy')
  local base = os.getenv("XDG_RUNTIME_DIR")
  if not base or base == "" then base = vim.fn.fnamemodify(vim.fn.tempname(), ":h") end
  local dir = base .. "/herdr-annotate-" .. vim.loop.getuid()
  vim.fn.mkdir(dir, "p", "0700")
  vim.fn.writefile(vim.split(vim.fn.getreg("z"), "\n"), dir .. "/selection")
  vim.fn.jobstart({ "herdr", "plugin", "action", "invoke", "annotate.capture" })
end, { desc = "Annotate in Herdr" })
```

Select text with the mouse or Visual mode. Then press `<leader>a` to open Herdr Annotate.
The file is read once and removed; a file older than 15 seconds is ignored.

LazyVim uses `Space` as `<leader>` by default. The mapping keeps mouse support and leaves normal Neovim commands unchanged.
