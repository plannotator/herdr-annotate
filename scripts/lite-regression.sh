#!/usr/bin/env bash
# Check the native Lite runtime against the goldens recorded from the retired Bun runtime.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"

for command in cargo python3; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "lite-regression: required command is unavailable: $command" >&2
    exit 2
  }
done

echo "== stage the native runtime"
bash "$root/scripts/stage-local.sh" >/dev/null

exec python3 "$root/scripts/lite-regression.py" \
  --root "$root" \
  --binary "$root/bin/herdr-annotate.exe" \
  "$@"
