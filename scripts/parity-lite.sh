#!/usr/bin/env bash
# Differential proof for the TypeScript and native Rust Lite implementations.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"

for command in bun cargo python3; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "parity-lite: required command is unavailable: $command" >&2
    exit 2
  }
done

echo "== stage native Lite"
bash "$root/lite-rs/scripts/stage-local.sh" >/dev/null

exec python3 "$root/scripts/parity-lite.py" \
  --root "$root" \
  --rust-binary "$root/lite-rs/bin/herdr-annotate.exe"
