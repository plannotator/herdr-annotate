#!/usr/bin/env bash
# Open a Ctrl-clicked file:// Markdown link in the installed Annotate plugin.
#
# Herdr runs this with the clicked URL and the focused pane in HERDR_PLUGIN_CONTEXT_JSON. It
# finds the `annotate` plugin's root, then runs that plugin's plannotator-tui with the same
# arguments, working directory and plugin id the old built-in open-link action used. The
# review pane therefore opens inside `annotate`, and feedback goes to the focused agent.
set -euo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
install_hint="herdr plugin install plannotator/herdr-annotate"

fail() {
  echo "$2" >&2
  "$herdr_bin" notification show "$1" --body "$2" >/dev/null 2>&1 || true
  exit 1
}

# Print the decoded value of a top-level JSON string field. `plugin list --plugin` returns one
# plugin, and these keys occur only at the plugin level, so the first match is the right one.
json_string() {
  printf '%s' "$2" | sed -nE "s/.*\"$1\":\"(([^\"\\\\]|\\\\.)*)\".*/\\1/p" | awk '
    {
      out = ""
      while ((i = index($0, "\\")) > 0) {
        c = substr($0, i + 1, 1)
        if (c == "n") c = "\n"; else if (c == "t") c = "\t"; else if (c == "u") exit 1
        out = out substr($0, 1, i - 1) c
        $0 = substr($0, i + 2)
      }
      print out $0
    }'
}

listing="$("$herdr_bin" plugin list --plugin annotate --json 2>/dev/null | tr -d '\n' || true)"
root="$(json_string plugin_root "$listing" || true)"

if [ -z "$root" ]; then
  fail "Annotate: plugin not installed" \
    "Ctrl-click needs the Annotate plugin. Install it with: $install_hint"
fi
case "$listing" in
  *'"enabled":true'*) ;;
  *) fail "Annotate: plugin disabled" \
       "Ctrl-click needs the Annotate plugin enabled. Enable it with: herdr plugin enable annotate" ;;
esac

tui="$root/bin/plannotator-tui.exe"
if [ ! -x "$tui" ]; then
  fail "Annotate: document review unavailable" \
    "Ctrl-click needs the Full Annotate plugin, which includes plannotator-tui. Install it with: $install_hint"
fi

cd "$root"
export HERDR_PLUGIN_ID=annotate HERDR_PLUGIN_ROOT="$root"
exec "$tui" herdr open
