#!/usr/bin/env bash
# Put the pinned native Lite runtime into bin/. Herdr runs this with cwd = plugin root.
#
# Modes, in order:
#   1. matching binary already installed                 -> exit 0
#   2. HERDR_ANNOTATE_BIN=/path/to/local/build is set    -> copy it
#   3. download the release asset and verify SHA256SUMS  -> install it
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(tr -d '[:space:]' < herdr-annotate.version)"
[ -n "$version" ] || { echo "herdr-annotate.version is empty" >&2; exit 1; }
mkdir -p bin
installed="$(cat bin/herdr-annotate.version 2>/dev/null || true)"

if [ -x bin/herdr-annotate.exe ] && [ "$installed" = "$version" ] && [ -z "${HERDR_ANNOTATE_BIN:-}" ]; then
  echo "herdr-annotate $version already installed"
  exit 0
fi

if [ -n "${HERDR_ANNOTATE_BIN:-}" ]; then
  [ -x "$HERDR_ANNOTATE_BIN" ] || { echo "HERDR_ANNOTATE_BIN is not executable: $HERDR_ANNOTATE_BIN" >&2; exit 1; }
  cp "$HERDR_ANNOTATE_BIN" bin/herdr-annotate.exe.tmp
  chmod +x bin/herdr-annotate.exe.tmp
  mv bin/herdr-annotate.exe.tmp bin/herdr-annotate.exe
  echo "$version" > bin/herdr-annotate.version
  echo "installed herdr-annotate from $HERDR_ANNOTATE_BIN (local build, stamped $version)"
  exit 0
fi

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)              target=aarch64-apple-darwin ;;
  Darwin/x86_64)             target=x86_64-apple-darwin ;;
  Linux/x86_64)              target=x86_64-unknown-linux-gnu ;;
  Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "no native Herdr Annotate Lite build for $(uname -s)/$(uname -m)" >&2; exit 1 ;;
esac

asset="herdr-annotate-$target"
base="https://github.com/plannotator/herdr-annotate/releases/download/rust-lite-v$version"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "need curl or wget" >&2
    return 1
  fi
}

echo "downloading $base/$asset"
fetch "$base/$asset" "$tmp/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"
expected="$(grep " $asset\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
[ -n "$expected" ] || { echo "$asset is not listed in $base/SHA256SUMS" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
fi
[ "$actual" = "$expected" ] || { echo "sha256 mismatch for $asset: expected $expected, got $actual" >&2; exit 1; }

chmod +x "$tmp/$asset"
mv "$tmp/$asset" bin/herdr-annotate.exe
echo "$version" > bin/herdr-annotate.version
echo "installed herdr-annotate $version ($target)"
