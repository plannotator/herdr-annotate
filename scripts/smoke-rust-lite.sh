#!/usr/bin/env bash
# Smoke-test the native Lite build in a disposable named Herdr session. This links the
# evaluation manifest, confirms its binary-backed entrypoints, renders the manager pane,
# then restores the machine's prior `annotate` install.
#
#   HERDR_SESSION=rust-lite-test bash scripts/smoke-rust-lite.sh
set -euo pipefail

[ -n "${HERDR_SESSION:-}" ] || { echo "set HERDR_SESSION to a disposable named session" >&2; exit 2; }
[ "$HERDR_SESSION" != default ] || { echo "refusing to run against the default session" >&2; exit 2; }

root="$(cd "$(dirname "$0")/.." && pwd)"
failures=0

plugin_json() { herdr plugin list --json | python3 -c '
import json,sys
for p in json.load(sys.stdin)["result"]["plugins"]:
    if p["plugin_id"] == "annotate":
        print(json.dumps(p)); break'; }
field() { python3 -c "import json,sys; p=json.load(sys.stdin); print(eval(sys.argv[1]))" "$1"; }
check() {
  if [ "$2" = "$3" ]; then
    echo "  ok   $1: $2"
  else
    echo "  FAIL $1: got '$2', want '$3'" >&2
    failures=$((failures+1))
  fi
}

before="$(plugin_json || true)"
restore() {
  echo "== restore"
  herdr plugin uninstall annotate >/dev/null 2>&1 || true
  if [ -z "$before" ]; then
    echo "  (nothing was installed)"
    return
  fi
  kind="$(printf '%s' "$before" | field "p['source']['kind']")"
  if [ "$kind" = github ]; then
    owner="$(printf '%s' "$before" | field "p['source']['owner']")"
    repo="$(printf '%s' "$before" | field "p['source']['repo']")"
    subdir="$(printf '%s' "$before" | field "p['source'].get('subdir') or ''")"
    commit="$(printf '%s' "$before" | field "p['source']['resolved_commit']")"
    herdr plugin install "$owner/$repo${subdir:+/$subdir}" --ref "$commit" --yes >/dev/null
    echo "  restored $owner/$repo${subdir:+/$subdir}@${commit:0:7}"
  else
    prior_root="$(printf '%s' "$before" | field "p['plugin_root']")"
    herdr plugin link "$prior_root" >/dev/null
    echo "  restored link $prior_root"
  fi
}
trap restore EXIT

echo "== build native Lite"
bash "$root/lite-rs/scripts/stage-local.sh" >/dev/null
binary="$root/rust/target/release/herdr-annotate"
check "binary version" "$("$binary" --version)" "herdr-annotate $(tr -d '[:space:]' < "$root/lite-rs/herdr-annotate.version")"

echo "== link native manifest"
herdr plugin link "$root/lite-rs" >/dev/null
installed="$(plugin_json)"
check "plugin root" "$(printf '%s' "$installed" | field "p['plugin_root']")" "$root/lite-rs"
actions="$(herdr plugin action list --plugin annotate | python3 -c '
import json,sys
print(",".join(sorted(a["action_id"] for a in json.load(sys.stdin)["result"]["actions"])))')"
check "actions" "$actions" "capture,copy-context,manage"
commands="$(printf '%s' "$installed" | python3 -c '
import json,sys
p=json.load(sys.stdin)
print(",".join(sorted(a["command"][0] for a in p["actions"])))')"
check "native action commands" "$commands" "./bin/herdr-annotate.exe,./bin/herdr-annotate.exe,./bin/herdr-annotate.exe"
check "bundled binary" "$("$root/lite-rs/bin/herdr-annotate.exe" --version)" "herdr-annotate $(tr -d '[:space:]' < "$root/lite-rs/herdr-annotate.version")"

echo "== manager pane renders in $HERDR_SESSION"
herdr plugin pane open --plugin annotate --entrypoint manager --placement overlay --focus >/dev/null
pane=""
for _ in $(seq 1 20); do
  pane="$(herdr pane list | python3 -c '
import json,sys
ps=[p for p in json.load(sys.stdin)["result"]["panes"] if p.get("label") == "Annotations"]
print(ps[-1]["pane_id"] if ps else "")')"
  [ -n "$pane" ] && break
  sleep 0.25
done
if [ -n "$pane" ] && herdr pane wait-output "$pane" --match "Annotations (" --timeout 8000 >/dev/null 2>&1; then
  echo "  ok   manager pane $pane rendered"
else
  echo "  FAIL manager pane did not render" >&2
  [ -n "$pane" ] && herdr pane read "$pane" >&2 || true
  herdr pane list >&2 || true
  herdr plugin log list --plugin annotate --limit 5 >&2 || true
  failures=$((failures+1))
fi
[ -n "$pane" ] && herdr plugin pane close "$pane" >/dev/null 2>&1 || true

echo "== result: $failures failure(s)"
[ "$failures" -eq 0 ]
