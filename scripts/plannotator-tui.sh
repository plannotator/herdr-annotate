#!/usr/bin/env bash
# Compatibility wrapper for released and local workflows. The current manifest invokes the
# staged binary directly; remove this wrapper after one compatibility release.
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
