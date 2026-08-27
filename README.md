![Herdr Annotate](assets/herdr-annotate.webp)

# herdr-annotate
Herdr Annotate adds comments to copied terminal text in [Herdr](https://github.com/herdrdev/herdr). It collects annotations as Markdown for use with any agent.

<p align="center">
  <a href="https://github.com/backnotprop/plannotator">
    <img src="./assets/star-plannotator.svg" width="280" alt="Like this? Star Plannotator">
  </a>
</p>

## Requirements

- Herdr 0.8.0 or later
- [Bun](https://bun.sh/)
- macOS, Linux, or Windows

On Linux, install `wl-clipboard`, `xclip`, or `xsel` for clipboard access.

On Windows, native Herdr plugin support is preview/best-effort. Bun must be on `PATH`. Clipboard access uses PowerShell; no extra clipboard package is required. The install, keybinding, configuration check, reload, and use instructions below also apply on Windows.

## Install

```sh
herdr plugin install plannotator/herdr-annotate
```

Add these key bindings to Herdr's config:

- macOS and Linux: `~/.config/herdr/config.toml`
- Windows: `%APPDATA%\herdr\config.toml`

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

Make sure that the configuration is valid:

```sh
herdr config check
```

Reload the configuration:

```sh
herdr server reload-config
```

## Use

1. Select terminal text in Herdr.
2. Press `Ctrl+B A`.
3. Enter a comment.
4. Press `Ctrl+S` to save the annotation.

Press `Ctrl+B M` to manage annotations. Press `Ctrl+B Shift+A` to copy all annotations as Markdown.

The manager shows the newest annotations first. Press `y` to copy one annotation or `c` to copy all annotations. A successful copy closes the manager and keeps the annotations saved.

Press `Shift+C` to copy all active annotations, archive the set, and clear the active list. Press `Tab` to browse archives. In the archive view, press `y` to copy a set, `u` to restore it, or `d` twice to permanently delete it.

## Selection limits

Herdr Annotate reads text that Herdr copies to the system clipboard. The plugin cannot read selection state from Neovim or another terminal application.

## Development

```sh
bun install
bun test
bun run typecheck
herdr plugin link "$PWD" --enabled
```

## Neovim integration

Add this visual-mode mapping to `~/.config/nvim/lua/config/keymaps.lua` for LazyVim, or to `init.lua`:

```lua
vim.keymap.set("x", "<leader>a", function()
  vim.cmd('normal! "+y')
  vim.fn.jobstart({
    "herdr",
    "plugin",
    "action",
    "invoke",
    "annotate.capture",
  })
end, { desc = "Annotate in Herdr" })
```

Select text with the mouse or Visual mode. Then press `<leader>a` to open Herdr Annotate.

LazyVim uses `Space` as `<leader>` by default. The mapping keeps mouse support and leaves normal Neovim commands unchanged.
