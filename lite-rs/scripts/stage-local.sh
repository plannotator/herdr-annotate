#!/usr/bin/env bash
# Build and stage the native runtime for `herdr plugin link`, which intentionally skips
# manifest build hooks. Run from any directory in the checkout.
set -euo pipefail

plugin_root="$(cd "$(dirname "$0")/.." && pwd)"
repository_root="$(cd "$plugin_root/.." && pwd)"
cargo build --manifest-path "$repository_root/rust/Cargo.toml" --release
HERDR_ANNOTATE_BIN="$repository_root/rust/target/release/herdr-annotate" \
  bash "$plugin_root/scripts/fetch-herdr-annotate.sh"
echo "staged $plugin_root/bin/herdr-annotate.exe"
echo "link with: herdr plugin link $plugin_root"
