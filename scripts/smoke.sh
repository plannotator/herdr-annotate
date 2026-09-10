#!/usr/bin/env bash
# Smoke-test the plugin the way users get it: fresh install, upgrade from an old commit, lite,
# and the lite -> full swap, exercising both fetched binaries and both panes in a disposable
# Herdr session. Restores whatever `annotate` install was present before it ran.
#
#   HERDR_SESSION=<disposable named session> bash scripts/smoke.sh [old-ref]
#
# Requires: herdr, python3, a running named session in HERDR_SESSION (never the default
# session; the pane tests open and close panes in it). The plugin registry is global, so
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
programs() { plugin_json | python3 -c "
import json,sys
print(','.join(sorted({a['command'][0] for a in json.load(sys.stdin)['actions']})))"; }
plugin_root() { plugin_json | field "p['plugin_root']"; }
# Lite installs from the lite/ subdirectory and shares the checkout's one staged bin/.
checkout_root() {
  local root; root="$(plugin_root)"
  case "$root" in */lite) printf '%s' "${root%/lite}" ;; *) printf '%s' "$root" ;; esac
}
tui_version() {
  local bin; bin="$(plugin_root)/bin/plannotator-tui.exe"
  [ -x "$bin" ] && "$bin" --version | awk '{print $2}' || echo none
}
# An old commit, and the Lite plugin root, carry only one of the two pins.
read_pin() { if [ -f "$1" ]; then tr -d '[:space:]' < "$1"; else echo none; fi; }
tui_pin() { read_pin "$(plugin_root)/plannotator-tui.version"; }
native_version() {
  local bin; bin="$(checkout_root)/bin/herdr-annotate.exe"
  [ -x "$bin" ] && "$bin" --version | awk '{print $2}' || echo none
}
native_pin() { read_pin "$(checkout_root)/herdr-annotate.version"; }
pane_id() { herdr pane list | python3 -c "
import json,sys
panes=[p for p in json.load(sys.stdin)['result']['panes'] if p.get('label')==sys.argv[1]]
print(panes[-1]['pane_id'] if panes else '')" "$1"; }
wait_pane() {
  local pane=""
  for _ in $(seq 1 20); do
    pane="$(pane_id "$1")"
    [ -n "$pane" ] && break
    sleep 0.25
  done
  printf '%s' "$pane"
}
# The manage action opens the manager as a popup, which `herdr pane list` does not report, so
# open it as an overlay with the same --cwd the action passes: the plugin root, which is what
# resolves the manifest's relative program for both variants.
manager_renders() {
  local pane result=FAIL
  herdr plugin pane open --plugin annotate --entrypoint manager --placement overlay \
    --cwd "$(plugin_root)" --focus >/dev/null 2>&1 || true
  pane="$(wait_pane Annotations)"
  if [ -n "$pane" ] && herdr pane wait-output "$pane" --match "Annotations (" --timeout 8000 >/dev/null 2>&1; then
    result=ok
  fi
  [ -n "$pane" ] && herdr plugin pane close "$pane" >/dev/null 2>&1 || true
  printf '%s' "$result"
}
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
check "actions" "$(actions)" "capture,copy-archive,copy-context,last,manage,open,open-link"
check "action programs" "$(programs)" "./bin/herdr-annotate.exe,./bin/plannotator-tui.exe"
check "review binary matches pin" "$(tui_version)" "$(tui_pin)"
check "native binary matches pin" "$(native_version)" "$(native_pin)"
check "manager pane" "$(manager_renders)" "ok"

echo "== review pane opens with --cwd (the #7 regression)"
work="$(mktemp -d)"; mkdir -p "$work/docs"; printf '# Smoke plan\n\nhello\n' > "$work/docs/plan.md"
herdr plugin pane open --plugin annotate --entrypoint doc --placement overlay --cwd "$work" \
  --env "PLANNOTATOR_TUI_FILE=$work/docs/plan.md" >/dev/null
pane="$(wait_pane Annotate)"
if [ -n "$pane" ] && herdr pane wait-output "$pane" --match "Smoke plan" --timeout 8000 >/dev/null 2>&1; then
  echo "  ok   pane $pane rendered the document"
else
  echo "  FAIL review pane did not render" >&2; failures=$((failures+1))
fi
[ -n "$pane" ] && herdr plugin pane close "$pane" >/dev/null 2>&1 || true
rm -rf "$work"

echo "== upgrade: $old_ref -> main"
install "$spec" --ref "$old_ref"
old_bin="$(tui_version)"; echo "  old review binary $old_bin (pin $(tui_pin))"
install "$spec"
check "review binary replaced on upgrade" "$(tui_version)" "$(tui_pin)"
check "native binary installed on upgrade" "$(native_version)" "$(native_pin)"
[ "$old_bin" != "$(tui_version)" ] && echo "  ok   review binary changed $old_bin -> $(tui_version)" || echo "  note old and new pins are equal; replacement not exercised"

echo "== fresh install: lite"
install "$spec/lite"
check "actions" "$(actions)" "capture,copy-archive,copy-context,manage"
check "action programs" "$(programs)" "../bin/herdr-annotate.exe"
check "no review binary" "$(tui_version)" "none"
check "native binary matches pin" "$(native_version)" "$(native_pin)"
check "manager pane" "$(manager_renders)" "ok"

echo "== swap: lite -> full"
install "$spec"
check "actions" "$(actions)" "capture,copy-archive,copy-context,last,manage,open,open-link"
check "review binary" "$(tui_version)" "$(tui_pin)"
check "native binary" "$(native_version)" "$(native_pin)"

echo "== result: $failures failure(s)"
[ "$failures" -eq 0 ]
