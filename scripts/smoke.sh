#!/usr/bin/env bash
# Smoke-test the plugin the way users get it: fresh install, upgrade from an old commit,
# lite, and the lite -> full swap, then exercise the review pane in a disposable Herdr
# session. Restores whatever `annotate` install was present before it ran.
#
#   HERDR_SESSION=<disposable named session> bash scripts/smoke.sh [old-ref]
#
# Requires: herdr, python3, a running named session in HERDR_SESSION (never the default
# session; the pane test opens and closes panes in it). The plugin registry is global, so
# this replaces the machine's `annotate` install while it runs and puts it back at the end.
set -euo pipefail

[ -n "${HERDR_SESSION:-}" ] || { echo "set HERDR_SESSION to a disposable named session" >&2; exit 2; }
[ "$HERDR_SESSION" != default ] || { echo "refusing to run against the default session" >&2; exit 2; }
old_ref="${1:-ed8593f639778644c64251979b3ecc165c0f8127}"   # the first shipped full manifest (plannotator-tui 0.3.0 pin)
spec=plannotator/herdr-annotate

plugin_json() { herdr plugin list --json | python3 -c "
import json,sys
for p in json.load(sys.stdin)['result']['plugins']:
    if p['plugin_id']=='annotate': print(json.dumps(p)); break"; }
field() { python3 -c "import json,sys; p=json.load(sys.stdin); print(eval(sys.argv[1]))" "$1"; }
actions() { herdr plugin action list --plugin annotate | python3 -c "
import json,sys; print(','.join(sorted(a['action_id'] for a in json.load(sys.stdin)['result']['actions'])))"; }
bin_version() {
  local root; root="$(plugin_json | field "p['plugin_root']")"
  local bin="$root/bin/plannotator-tui"; [ -x "$bin" ] && "$bin" --version | awk '{print $2}' || echo none
}
pin() { local root; root="$(plugin_json | field "p['plugin_root']")"; tr -d '[:space:]' < "$root/plannotator-tui.version" 2>/dev/null || echo none; }
check() { if [ "$2" = "$3" ]; then echo "  ok   $1: $2"; else echo "  FAIL $1: got '$2', want '$3'" >&2; failures=$((failures+1)); fi; }
install() { herdr plugin uninstall annotate >/dev/null 2>&1 || true; herdr plugin install "$@" --yes >/dev/null; }
failures=0

# Remember what was installed so it can be restored.
before="$(plugin_json || true)"
restore() {
  echo "== restore"
  herdr plugin uninstall annotate >/dev/null 2>&1 || true
  if [ -z "$before" ]; then echo "  (nothing was installed)"; return; fi
  local kind; kind="$(printf '%s' "$before" | field "p['source']['kind']")"
  if [ "$kind" = github ]; then
    local owner repo subdir commit
    owner="$(printf '%s' "$before" | field "p['source']['owner']")"
    repo="$(printf '%s' "$before" | field "p['source']['repo']")"
    subdir="$(printf '%s' "$before" | field "p['source'].get('subdir') or ''")"
    commit="$(printf '%s' "$before" | field "p['source']['resolved_commit']")"
    herdr plugin install "$owner/$repo${subdir:+/$subdir}" --ref "$commit" --yes >/dev/null && echo "  restored $owner/$repo${subdir:+/$subdir}@${commit:0:7}"
  else
    local root; root="$(printf '%s' "$before" | field "p['plugin_root']")"
    herdr plugin link "$root" >/dev/null && echo "  restored link $root"
  fi
}
trap restore EXIT

echo "== fresh install: full"
install "$spec"
check "actions" "$(actions)" "capture,copy-context,last,manage,open,open-link"
check "binary matches pin" "$(bin_version)" "$(pin)"

echo "== review pane opens with --cwd (the #7 regression)"
work="$(mktemp -d)"; mkdir -p "$work/docs"; printf '# Smoke plan\n\nhello\n' > "$work/docs/plan.md"
before_panes="$(herdr pane list | python3 -c "import json,sys; print(len(json.load(sys.stdin)['result']['panes']))")"
herdr plugin pane open --plugin annotate --entrypoint doc --placement overlay --cwd "$work" \
  --env "PLANNOTATOR_TUI_FILE=$work/docs/plan.md" >/dev/null
pane="$(herdr pane list | python3 -c "
import json,sys; ps=[p for p in json.load(sys.stdin)['result']['panes'] if p.get('label')=='Annotate']; print(ps[-1]['pane_id'] if ps else '')")"
if [ -n "$pane" ] && herdr pane wait-output "$pane" --match "Smoke plan" --timeout 8000 >/dev/null 2>&1; then
  echo "  ok   pane $pane rendered the document"
else
  echo "  FAIL review pane did not render" >&2; failures=$((failures+1))
fi
[ -n "$pane" ] && herdr plugin pane close "$pane" >/dev/null 2>&1 || true
rm -rf "$work"

echo "== upgrade: $old_ref -> main"
install "$spec" --ref "$old_ref"
old_bin="$(bin_version)"; echo "  old binary $old_bin (pin $(pin))"
install "$spec"
check "binary replaced on upgrade" "$(bin_version)" "$(pin)"
[ "$old_bin" != "$(bin_version)" ] && echo "  ok   binary changed $old_bin -> $(bin_version)" || echo "  note old and new pins are equal; replacement not exercised"

echo "== fresh install: lite"
install "$spec/lite"
check "actions" "$(actions)" "capture,copy-context,manage"
check "no binary" "$(bin_version)" "none"

echo "== swap: lite -> full"
install "$spec"
check "actions" "$(actions)" "capture,copy-context,last,manage,open,open-link"
check "binary" "$(bin_version)" "$(pin)"

echo "== result: $failures failure(s)"
[ "$failures" -eq 0 ]
