#!/usr/bin/env bash
# The review pane runs with the folder under review as cwd. Resolve the staged binary from this
# script's plugin-root location instead.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
if [ -x "$root/bin/plannotator-tui.exe" ]; then
  exec "$root/bin/plannotator-tui.exe" "$@"
fi
msg="plannotator-tui is not installed. Reinstall the plugin: herdr plugin install plannotator/herdr-annotate"
echo "$msg" >&2
if [ -n "${HERDR_BIN_PATH:-}" ]; then
  "$HERDR_BIN_PATH" notification show "Annotate: review pane unavailable" --body "$msg" >/dev/null 2>&1 || true
fi
exit 1
