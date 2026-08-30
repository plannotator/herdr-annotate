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

- Herdr 0.8.0 or later
- [Bun](https://bun.sh/)
- macOS, Linux, or Windows

On Linux, install `wl-clipboard`, `xclip`, or `xsel` for clipboard access.

On Windows, native Herdr plugin support is preview/best-effort. Bun must be on `PATH`. Clipboard access uses PowerShell; no extra clipboard package is required. The install, keybinding, configuration check, reload, and use instructions below also apply on Windows.

## Install

Pick one. Installing the other later just swaps it (same plugin id).

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
| `Ctrl+B M` | manage · `y` copy one · `c` copy all · `Shift+C` copy and archive · `Tab` archives (`y` copy · `u` restore · `d d` delete) |

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

Over SSH or `herdr --remote`, the plugin runs on the **server**, and two things get in the way:
Herdr's default copy-on-select clears the selection on mouse-up, and the prefix keypress
clears whatever selection remains before a bound action runs
([herdrdev/herdr#3380](https://github.com/herdrdev/herdr/issues/3380)). A headless server also
has no clipboard for the plugin to fall back to.

What works today:

1. On the server, keep the selection after mouse-up:

   ```toml
   # remote server: ~/.config/herdr/config.toml
   [ui]
   copy_on_select = false   # the selection stays; copy explicitly with Ctrl+C
   ```

2. Trigger the action **without a keypress in Herdr**, while the selection is still
   highlighted. From your laptop, bound to any key in your terminal or OS:

   ```sh
   ssh <host> herdr plugin action invoke annotate.capture
   # named session on the server: ssh <host> HERDR_SESSION=<name> herdr plugin action invoke annotate.capture
   ```

   The action reads the focused pane's selection through Herdr's API, which never touches the
   keyboard path, so the text arrives. Verified: the same selection gives `selected_text` this
   way and nothing through `prefix+a`.

3. In Neovim, use the mapping below; it hands the selection over in a file.

Server-side key bindings and `herdr --remote <host> --remote-keybindings server` are still
needed for the manager (`prefix+m`) and other plugin keys; without the flag, `herdr --remote`
uses your local keys and drops plugin bindings. `prefix+a` itself will work once
herdrdev/herdr#3380 is fixed.

## Selection limits

Herdr Annotate reads text that Herdr copies to the system clipboard. The plugin cannot read selection state from Neovim or another terminal application.

## Development

```sh
bun install
bun test
bun run typecheck
herdr plugin link "$PWD"
```

To test a local plannotator-tui build instead of the pinned release, put it in `bin/`
before linking: `PLANNOTATOR_TUI_BIN=/path/to/plannotator-tui bash scripts/fetch-plannotator-tui.sh`.
`herdr plugin link` replaces any existing `annotate` link; link the other directory to switch back.

Before a release, `HERDR_SESSION=<disposable session> bash scripts/smoke.sh` installs fresh, upgrades
from the first shipped commit, installs lite, swaps to full, and opens the review pane, then restores
whatever was installed.

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
