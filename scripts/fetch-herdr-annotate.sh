#!/usr/bin/env bash
# Put the pinned native Lite runtime into bin/. Herdr runs this with cwd = plugin root.
#
# Modes, in order:
#   1. matching binary already installed and runnable   -> exit 0
#   2. HERDR_ANNOTATE_BIN=/path/to/local/build is set    -> copy it
#   3. download the release asset and verify SHA256SUMS  -> install it
#   4. fallback to local cargo build if available        -> compile and install it
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(tr -d '[:space:]' < herdr-annotate.version)"
[ -n "$version" ] || { echo "herdr-annotate.version is empty" >&2; exit 1; }
mkdir -p bin
target_file="bin/herdr-annotate.target"
installed="$(cat bin/herdr-annotate.version 2>/dev/null || true)"
installed_target="$(cat "$target_file" 2>/dev/null || true)"

detect_target() {
  case "$(uname -s)/$(uname -m)" in
    Darwin/arm64)              echo "aarch64-apple-darwin" ;;
    Darwin/x86_64)             echo "x86_64-apple-darwin" ;;
    Linux/x86_64)              echo "x86_64-unknown-linux-gnu" ;;
    Linux/aarch64|Linux/arm64) echo "aarch64-unknown-linux-gnu" ;;
    MINGW*/*64*|MSYS*/*64*|CYGWIN*/*64*) echo "x86_64-pc-windows-msvc" ;;
    MINGW*/*arm64*|MSYS*/*arm64*|CYGWIN*/*arm64*|MINGW*/*aarch64*|MSYS*/*aarch64*|CYGWIN*/*aarch64*) echo "aarch64-pc-windows-msvc" ;;
    *) echo "unknown" ;;
  esac
}

target="$(detect_target)"
if [ "$target" = "unknown" ]; then
  if command -v node >/dev/null 2>&1; then
    target="$(node -e '
      const p = process.platform, a = process.arch;
      if (p === "darwin") console.log(a === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin");
      else if (p === "linux") console.log(a === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu");
      else if (p === "win32") console.log(a === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc");
      else console.log("unknown");
    ' 2>/dev/null || echo "unknown")"
  elif command -v bun >/dev/null 2>&1; then
    target="$(bun -e '
      const p = process.platform, a = process.arch;
      if (p === "darwin") console.log(a === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin");
      else if (p === "linux") console.log(a === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu");
      else if (p === "win32") console.log(a === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc");
      else console.log("unknown");
    ' 2>/dev/null || echo "unknown")"
  fi
fi

fallback_build() {
  if [ -f "rust/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "download failed or no prebuilt available; falling back to cargo build" >&2
    if cargo build --manifest-path rust/Cargo.toml --release; then
      local built="rust/target/release/herdr-annotate"
      [ -f "$built" ] || built="rust/target/release/herdr-annotate.exe"
      if [ -f "$built" ]; then
        cp "$built" bin/herdr-annotate.exe.tmp
        chmod +x bin/herdr-annotate.exe.tmp
        mv bin/herdr-annotate.exe.tmp bin/herdr-annotate.exe
        echo "$version" > bin/herdr-annotate.version
        if [ "$target" != "unknown" ]; then
          echo "$target" > "$target_file"
        fi
        echo "installed herdr-annotate from fallback build (stamped $version ($target))"
        return 0
      fi
    fi
  fi
  return 1
}

is_runnable_and_matches() {
  local bin="bin/herdr-annotate.exe"
  [ -x "$bin" ] || return 1
  if [ -n "$installed_target" ] && [ "$target" != "unknown" ] && [ "$installed_target" != "$target" ]; then
    return 1
  fi
  local bin_ver
  bin_ver="$("$bin" --version 2>/dev/null)" || return 1
  [ "$bin_ver" = "herdr-annotate $version" ] || return 1
  return 0
}

if [ -z "${HERDR_ANNOTATE_BIN:-}" ] && [ "$installed" = "$version" ] && is_runnable_and_matches; then
  if [ ! -f "$target_file" ] && [ "$target" != "unknown" ]; then
    echo "$target" > "$target_file"
  fi
  echo "herdr-annotate $version already installed"
  exit 0
fi

if [ -n "${HERDR_ANNOTATE_BIN:-}" ]; then
  [ -x "$HERDR_ANNOTATE_BIN" ] || { echo "HERDR_ANNOTATE_BIN is not executable: $HERDR_ANNOTATE_BIN" >&2; exit 1; }
  cp "$HERDR_ANNOTATE_BIN" bin/herdr-annotate.exe.tmp
  chmod +x bin/herdr-annotate.exe.tmp
  mv bin/herdr-annotate.exe.tmp bin/herdr-annotate.exe
  echo "$version" > bin/herdr-annotate.version
  if [ "$target" != "unknown" ]; then
    echo "$target" > "$target_file"
  fi
  echo "installed herdr-annotate from $HERDR_ANNOTATE_BIN (local build, stamped $version)"
  exit 0
fi

if [ "$target" = "unknown" ]; then
  if fallback_build; then
    exit 0
  fi
  echo "no native Herdr Annotate Lite build for $(uname -s)/$(uname -m) and fallback build failed" >&2
  exit 1
fi

case "$target" in
  *windows*) asset="herdr-annotate-$target.exe" ;;
  *)         asset="herdr-annotate-$target" ;;
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

download_and_install() {
  echo "downloading $base/$asset"
  fetch "$base/$asset" "$tmp/$asset" || return 1
  fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || return 1
  expected="$(grep " $asset\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
  [ -n "$expected" ] || { echo "$asset is not listed in $base/SHA256SUMS" >&2; return 1; }
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
  fi
  [ "$actual" = "$expected" ] || { echo "sha256 mismatch for $asset: expected $expected, got $actual" >&2; return 1; }
  chmod +x "$tmp/$asset"
  mv "$tmp/$asset" bin/herdr-annotate.exe
  echo "$version" > bin/herdr-annotate.version
  echo "$target" > "$target_file"
  echo "installed herdr-annotate $version ($target)"
  return 0
}

if ! download_and_install; then
  if fallback_build; then
    exit 0
  fi
  echo "failed to install herdr-annotate for $target" >&2
  exit 1
fi
