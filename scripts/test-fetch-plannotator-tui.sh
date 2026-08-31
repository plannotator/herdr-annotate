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
printf 'plannotator-tui 0.6.0\n'
EOF
chmod +x "$source_binary"
printf 'old destination' > "$plugin_root/bin/plannotator-tui.exe"
chmod +x "$plugin_root/bin/plannotator-tui.exe"
printf 'old-version' > "$plugin_root/bin/plannotator-tui.version"

PLANNOTATOR_TUI_BIN="$source_binary" bash "$plugin_root/scripts/fetch-plannotator-tui.sh"
cmp "$source_binary" "$plugin_root/bin/plannotator-tui.exe"
test "$(cat "$plugin_root/bin/plannotator-tui.version")" = 0.6.0
test ! -e "$plugin_root/bin/plannotator-tui"
test "$("$plugin_root/bin/plannotator-tui.exe" --version)" = "plannotator-tui 0.6.0"
test "$(bash "$plugin_root/scripts/plannotator-tui.sh" --version)" = "plannotator-tui 0.6.0"

output="$(bash "$plugin_root/scripts/fetch-plannotator-tui.sh")"
case "$output" in
  *"already installed"*) ;;
  *) echo "idempotent fetch did not short-circuit: $output" >&2; exit 1 ;;
esac
