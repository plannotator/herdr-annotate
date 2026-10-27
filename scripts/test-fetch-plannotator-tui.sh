#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT
plugin_root="$test_root/plugin root with spaces"
source_root="$test_root/source binary with spaces"
mkdir -p "$plugin_root/scripts" "$plugin_root/bin" "$source_root"
cp "$repository_root/scripts/fetch-plannotator-tui.sh" "$plugin_root/scripts/"
cp "$repository_root/scripts/plannotator-tui.sh" "$plugin_root/scripts/"
cp "$repository_root/plannotator-tui.version" "$plugin_root/"

source_binary="$source_root/plannotator-tui local"
cat > "$source_binary" <<'EOF'
#!/usr/bin/env sh
printf 'plannotator-tui 0.9.5\n'
EOF
chmod +x "$source_binary"
printf 'old destination' > "$plugin_root/bin/plannotator-tui.exe"
chmod +x "$plugin_root/bin/plannotator-tui.exe"
printf 'old-version' > "$plugin_root/bin/plannotator-tui.version"

PLANNOTATOR_TUI_BIN="$source_binary" bash "$plugin_root/scripts/fetch-plannotator-tui.sh"
cmp "$source_binary" "$plugin_root/bin/plannotator-tui.exe"
test "$(cat "$plugin_root/bin/plannotator-tui.version")" = 0.9.5
test ! -e "$plugin_root/bin/plannotator-tui"
test "$("$plugin_root/bin/plannotator-tui.exe" --version)" = "plannotator-tui 0.9.5"
test "$(bash "$plugin_root/scripts/plannotator-tui.sh" --version)" = "plannotator-tui 0.9.5"

output="$(bash "$plugin_root/scripts/fetch-plannotator-tui.sh")"
case "$output" in
  *"already installed"*) ;;
  *) echo "idempotent fetch did not short-circuit: $output" >&2; exit 1 ;;
esac

# A release served without the network: curl is replaced on PATH by a stub that answers from
# $release_root by file name and logs every URL it was asked for.
release_root="$test_root/release with spaces"
fake_path="$test_root/fake path"
mkdir -p "$release_root" "$fake_path"
cat > "$fake_path/curl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
out="" url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o|--retry) [ "$1" = -o ] && out="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
printf '%s\n' "$url" >> "$FAKE_CURL_LOG"
cp "$FAKE_RELEASE_ROOT/${url##*/}" "$out"
STUB
chmod +x "$fake_path/curl"
curl_log="$test_root/curl.log"
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }
publish() {
  : > "$release_root/SHA256SUMS"
  for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu x86_64-unknown-linux-musl aarch64-unknown-linux-gnu; do
    cp "$1" "$release_root/plannotator-tui-$target"
    printf '%s  %s\n' "$(sha256 "$1")" "plannotator-tui-$target" >> "$release_root/SHA256SUMS"
  done
}
fetch_offline() {
  : > "$curl_log"
  PATH="$fake_path:$PATH" FAKE_CURL_LOG="$curl_log" FAKE_RELEASE_ROOT="$release_root" \
    bash "$plugin_root/scripts/fetch-plannotator-tui.sh"
}
# A binary built for the other OS, as a plugin directory synced from another machine carries:
# this host's kernel refuses to execute it.
install_foreign() {
  case "$(uname -s)" in Darwin) magic='\177ELF' ;; *) magic='\317\372\355\376' ;; esac
  printf '%b' "$magic"'\002\001\001\000\000\000\000\000\000\000\000\000' > "$1"
  chmod +x "$1"
}

version="$(tr -d '[:space:]' < "$repository_root/plannotator-tui.version")"
release_binary="$test_root/release plannotator-tui"
cat > "$release_binary" <<RELEASE
#!/usr/bin/env sh
printf 'plannotator-tui $version\n'
# release fixture
RELEASE
chmod +x "$release_binary"
publish "$release_binary"

# A matching stamp over a binary that cannot run here is not "already installed": the verified
# release replaces it.
install_foreign "$plugin_root/bin/plannotator-tui.exe"
output="$(fetch_offline)"
case "$output" in
  *"already installed"*) echo "an unrunnable binary short-circuited: $output" >&2; exit 1 ;;
esac
cmp "$release_binary" "$plugin_root/bin/plannotator-tui.exe"
test "$(cat "$plugin_root/bin/plannotator-tui.version")" = "$version"
grep -qx "https://github.com/plannotator/plannotator-tui/releases/download/v$version/SHA256SUMS" "$curl_log"

output="$(fetch_offline)"
case "$output" in
  *"already installed"*) ;;
  *) echo "a runnable reinstalled binary did not short-circuit: $output" >&2; exit 1 ;;
esac
test ! -s "$curl_log"
