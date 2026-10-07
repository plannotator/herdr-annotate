# Open a Ctrl-clicked file:// Markdown link in the installed Annotate plugin, on Windows.
#
# The Windows counterpart of open-link.sh. Herdr runs this with the clicked URL and the focused
# pane in HERDR_PLUGIN_CONTEXT_JSON. It finds the `annotate` plugin's root, then runs that
# plugin's plannotator-tui with the same arguments, working directory and plugin id the old
# built-in open-link action of Windows Full used. The review pane therefore opens inside
# `annotate`, and feedback goes to the focused agent.

# Continue, not Stop: Windows PowerShell turns any stderr from a native command into a
# terminating error under Stop. Every outcome below is checked explicitly instead.
$ErrorActionPreference = "Continue"

$herdr = if ([string]::IsNullOrEmpty($env:HERDR_BIN_PATH)) { "herdr" } else { $env:HERDR_BIN_PATH }
$installHint = "herdr plugin install plannotator/herdr-annotate/windows-full"

function Fail {
  param([string]$Title, [string]$Body)
  [Console]::Error.WriteLine($Body)
  try { & $herdr notification show $Title --body $Body *> $null } catch { }
  exit 1
}

# Herdr writes UTF-8; Windows PowerShell would otherwise decode it with the OEM code page.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$plugin = $null
try {
  $listing = (& $herdr plugin list --plugin annotate --json 2> $null) -join "`n"
  $plugin = @(($listing | ConvertFrom-Json).result.plugins) |
    Where-Object { $null -ne $_ -and $_.plugin_id -ceq "annotate" } |
    Select-Object -First 1
} catch {
  $plugin = $null
}

if ($null -eq $plugin -or [string]::IsNullOrEmpty($plugin.plugin_root)) {
  Fail "Annotate: plugin not installed" "Ctrl-click needs the Annotate plugin. Install it with: $installHint"
}
if (-not $plugin.enabled) {
  Fail "Annotate: plugin disabled" "Ctrl-click needs the Annotate plugin enabled. Enable it with: herdr plugin enable annotate"
}

# .NET rather than Join-Path/Test-Path: the root may be an extended-length \\?\ path.
$root = [string]$plugin.plugin_root
$tui = [System.IO.Path]::Combine($root, "bin", "plannotator-tui.exe")
if (-not [System.IO.File]::Exists($tui)) {
  Fail "Annotate: document review unavailable" "Ctrl-click needs Annotate Windows Full, which includes plannotator-tui. Install it with: $installHint"
}

$env:HERDR_PLUGIN_ID = "annotate"
$env:HERDR_PLUGIN_ROOT = $root
# Herdr ran the old action from the plugin root; do the same, best effort.
try { [System.IO.Directory]::SetCurrentDirectory($root) } catch { }
try { Set-Location -LiteralPath $root } catch { }
& $tui herdr open
exit $LASTEXITCODE
