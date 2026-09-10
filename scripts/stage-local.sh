#!/usr/bin/env bash
# Build and stage the native runtime for `herdr plugin link`, which intentionally skips
# manifest build hooks. Run from any directory in the checkout.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --manifest-path "$root/rust/Cargo.toml" --release
HERDR_ANNOTATE_BIN="$root/rust/target/release/herdr-annotate" \
  bash "$root/scripts/fetch-herdr-annotate.sh"
echo "staged $root/bin/herdr-annotate.exe"
