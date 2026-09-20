#!/usr/bin/env bash
# Put the pinned plannotator-tui release into bin/. Run by Herdr as a plugin build step
# (cwd = plugin root) and by hand for local testing.
#
#   plannotator-tui.version        the release to install (one line, e.g. 0.1.0)
#   bin/plannotator-tui.exe        the binary
#   bin/plannotator-tui.version    what is installed; matching the pin means nothing to do
#
# Modes, in order:
#   1. already installed at the pinned version         -> exit 0
#   2. PLANNOTATOR_TUI_BIN=/path/to/binary is set      -> copy it (local testing, no download)
#   3. otherwise download the release asset for this platform and verify its sha256
#
# A failed download never fails the plugin install: the TypeScript tools keep working and the
# review actions explain what to do. Only a bad PLANNOTATOR_TUI_BIN is an error.
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(tr -d '[:space:]' < plannotator-tui.version)"
[ -n "$version" ] || { echo "plannotator-tui.version is empty" >&2; exit 1; }
mkdir -p bin
destination="bin/plannotator-tui.exe"
stamp="bin/plannotator-tui.version"
target_file="bin/plannotator-tui.target"
installed="$(cat "$stamp" 2>/dev/null || true)"
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

is_runnable_and_matches() {
  [ -x "$destination" ] || return 1
  if [ -n "$installed_target" ] && [ "$target" != "unknown" ] && [ "$installed_target" != "$target" ]; then
    return 1
  fi
  local bin_ver
  bin_ver="$("$destination" --version 2>/dev/null)" || return 1
  [ "$bin_ver" = "plannotator-tui $version" ] || return 1
  return 0
}

if [ -x "$destination" ] && [ "$installed" = "$version" ] && is_runnable_and_matches && [ -z "${PLANNOTATOR_TUI_BIN:-}" ]; then
  if [ ! -f "$target_file" ] && [ "$target" != "unknown" ]; then
    echo "$target" > "$target_file"
  fi
  echo "plannotator-tui $version already installed"
  exit 0
fi

if [ -n "${PLANNOTATOR_TUI_BIN:-}" ]; then
  [ -x "$PLANNOTATOR_TUI_BIN" ] || { echo "PLANNOTATOR_TUI_BIN is not an executable: $PLANNOTATOR_TUI_BIN" >&2; exit 1; }
  rm -f "$destination"
  cp "$PLANNOTATOR_TUI_BIN" "$destination"
  chmod +x "$destination"
  printf '%s' "$version" > "$stamp"
  if [ "$target" != "unknown" ]; then
    echo "$target" > "$target_file"
  fi
  echo "installed plannotator-tui from $PLANNOTATOR_TUI_BIN (local build, stamped $version)"
  exit 0
fi

if [ "$target" = "unknown" ]; then
  echo "warning: no plannotator-tui build for $(uname -s)/$(uname -m); the review pane is unavailable" >&2
  exit 0
fi

asset="plannotator-tui-$target"
base="https://github.com/plannotator/plannotator-tui/releases/download/v$version"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "need curl or wget" >&2; return 1
  fi
}

echo "downloading $base/$asset"
give_up() { echo "warning: $1 — the review pane is unavailable until the plugin is reinstalled" >&2; exit 0; }
fetch "$base/$asset" "$tmp/$asset"       || give_up "download failed: $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || give_up "download failed: $base/SHA256SUMS"

expected="$(grep " $asset\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
[ -n "$expected" ] || give_up "$asset is not listed in $base/SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
fi
[ "$actual" = "$expected" ] || give_up "sha256 mismatch for $asset: expected $expected, got $actual"

chmod +x "$tmp/$asset"
rm -f "$destination"
cp "$tmp/$asset" "$destination"
chmod +x "$destination"
printf '%s' "$version" > "$stamp"
echo "$target" > "$target_file"
echo "installed plannotator-tui $version ($target)"
