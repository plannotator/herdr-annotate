#!/usr/bin/env bash
# The native Lite fetcher: local override, idempotence, and the two fatal inputs. Unlike the
# plannotator-tui fetcher, a failure here is fatal: the annotation tools are the binary.
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT
plugin_root="$test_root/plugin root with spaces"
source_root="$test_root/source binary with spaces"
mkdir -p "$plugin_root/scripts" "$plugin_root/bin" "$source_root"
cp "$repository_root/scripts/fetch-herdr-annotate.sh" "$plugin_root/scripts/"
cp "$repository_root/herdr-annotate.version" "$plugin_root/"
version="$(tr -d '[:space:]' < "$repository_root/herdr-annotate.version")"

source_binary="$source_root/herdr-annotate local"
cat > "$source_binary" <<SOURCE
#!/usr/bin/env sh
printf 'herdr-annotate $version\n'
SOURCE
chmod +x "$source_binary"
printf 'old destination' > "$plugin_root/bin/herdr-annotate.exe"
chmod +x "$plugin_root/bin/herdr-annotate.exe"
printf 'old-version' > "$plugin_root/bin/herdr-annotate.version"

HERDR_ANNOTATE_BIN="$source_binary" bash "$plugin_root/scripts/fetch-herdr-annotate.sh"
cmp "$source_binary" "$plugin_root/bin/herdr-annotate.exe"
test "$(cat "$plugin_root/bin/herdr-annotate.version")" = "$version"
test ! -e "$plugin_root/bin/herdr-annotate"
test "$("$plugin_root/bin/herdr-annotate.exe" --version)" = "herdr-annotate $version"

output="$(bash "$plugin_root/scripts/fetch-herdr-annotate.sh")"
case "$output" in
  *"already installed"*) ;;
  *) echo "idempotent fetch did not short-circuit: $output" >&2; exit 1 ;;
esac

before="$(cat "$plugin_root/bin/herdr-annotate.exe")"
if HERDR_ANNOTATE_BIN="$test_root/missing override" bash "$plugin_root/scripts/fetch-herdr-annotate.sh" 2>"$test_root/missing.err"; then
  echo "a missing local override exited successfully" >&2; exit 1
fi
grep -q "HERDR_ANNOTATE_BIN is not executable" "$test_root/missing.err"
test "$(cat "$plugin_root/bin/herdr-annotate.exe")" = "$before"

printf '' > "$plugin_root/herdr-annotate.version"
if bash "$plugin_root/scripts/fetch-herdr-annotate.sh" 2>"$test_root/empty.err"; then
  echo "an empty version pin exited successfully" >&2; exit 1
fi
grep -q "herdr-annotate.version is empty" "$test_root/empty.err"
test "$(cat "$plugin_root/bin/herdr-annotate.exe")" = "$before"
