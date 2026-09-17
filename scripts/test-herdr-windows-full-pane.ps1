# The Windows Full review pane, end to end, against a pinned Herdr release in an isolated
# config/state/socket so the machine's own server and session are never touched.
#
# What this proves that no manifest or link test can: plannotator-tui starts as the pane
# process, renders a document from a review folder outside the checkout, quits on `q` with
# status zero, and leaves nothing running. The fixture marker is generated per run, so a
# stale buffer cannot satisfy the render assertion.
#
# The binary under test is fetched by the variant's own build command from the real release,
# with both override variables absent, and its location and stamp are asserted before use.
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$temporaryBase = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { $env:TEMP } else { $env:RUNNER_TEMP }
$testRoot = Join-Path $temporaryBase ("herdr windows full pane " + [guid]::NewGuid())

$herdrVersion = "0.9.0"
$herdrSha256 = "b4508c445de1c1a68c760a01735da2aba2fa214b2aafd4b07f732e49b2a64b11"

# Spaces in both roots: the review folder and the plugin root are separate path cases, and
# the pane receives one as cwd while the program is resolved from the other.
$checkout = Join-Path $testRoot "checkout with spaces"
$variantRoot = Join-Path $checkout "windows-full"
$review = Join-Path $testRoot "review folder"
$marker = "HERDRFULL-" + ([guid]::NewGuid().ToString("N").Substring(0, 12)).ToUpperInvariant()
$fixture = "fixture $marker.md"

$isolatedNames = @(
  "XDG_CONFIG_HOME",
  "XDG_STATE_HOME",
  "HERDR_CONFIG_PATH",
  "HERDR_SESSION",
  "HERDR_SOCKET_PATH",
  "HERDR_CLIENT_SOCKET_PATH",
  "PLANNOTATOR_TUI_BIN",
  "PLANNOTATOR_TUI_RELEASE_BASE"
)
$oldEnvironment = @{}
foreach ($name in $isolatedNames) {
  $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

$herdrExecutable = $null

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Invoke-Herdr {
  param([string[]]$Arguments)
  $output = & $script:herdrExecutable @Arguments *>&1 | Out-String
  $output
}

function Get-TuiProcessIds {
  # Scoped to the staged executable so a developer's own plannotator-tui is never counted.
  param([string]$Path)
  @(
    Get-Process plannotator-tui -ErrorAction SilentlyContinue |
      Where-Object {
        $candidate = $null
        try { $candidate = $_.Path } catch { $candidate = $null }
        $null -ne $candidate -and $candidate -ceq $Path
      } |
      Select-Object -ExpandProperty Id
  )
}

try {
  New-Item -ItemType Directory -Force $review | Out-Null
  New-Item -ItemType Directory -Force (Join-Path $variantRoot "scripts") | Out-Null
  New-Item -ItemType Directory -Force (Join-Path $checkout "scripts") | Out-Null
  Set-Content -LiteralPath (Join-Path $review $fixture) -Encoding utf8 -Value @(
    "# $marker",
    "",
    "Unique fixture content for this run: $marker"
  )

  # A staged checkout rather than the working tree: the build must work from a relocated
  # copy, which is what an installed plugin is.
  foreach ($name in @("plannotator-tui.version", "herdr-annotate.version")) {
    Copy-Item -LiteralPath (Join-Path $repositoryRoot $name) -Destination $checkout
  }
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts\fetch-plannotator-tui.ps1") `
    -Destination (Join-Path $checkout "scripts")
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts\fetch-herdr-annotate.ps1") `
    -Destination (Join-Path $checkout "scripts")
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "windows-full\herdr-plugin.toml") `
    -Destination $variantRoot
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "windows-full\scripts\fetch-plannotator-tui.ps1") `
    -Destination (Join-Path $variantRoot "scripts")

  # Both build entries from the manifest, run the way Herdr runs them: from the plugin root,
  # with no override variables set, against the real release.
  Push-Location $variantRoot
  try {
    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass `
      -File "..\scripts\fetch-herdr-annotate.ps1" | Out-Null
    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass `
      -File "scripts\fetch-plannotator-tui.ps1" | Out-Null
  } finally {
    Pop-Location
  }

  $tui = Join-Path $variantRoot "bin\plannotator-tui.exe"
  $native = Join-Path $checkout "bin\herdr-annotate.exe"
  Assert-True (Test-Path -LiteralPath $tui -PathType Leaf) "the variant build staged no plannotator-tui.exe"
  Assert-True (Test-Path -LiteralPath $native -PathType Leaf) "the native build staged no herdr-annotate.exe"
  $pin = (Get-Content -LiteralPath (Join-Path $checkout "plannotator-tui.version") -Raw).Trim()
  $stamp = (Get-Content -LiteralPath (Join-Path $variantRoot "bin\plannotator-tui.version") -Raw).Trim()
  Assert-True ($stamp -ceq $pin) "staged stamp $stamp does not match the pin $pin"
  $reported = (& $tui --version *>&1 | Out-String).Trim()
  Assert-True ($reported -match [regex]::Escape($pin)) "staged TUI reports '$reported', expected $pin"
  Write-Output "staged plannotator-tui $pin at windows-full/bin, native runtime one level up"

  $archive = Join-Path $testRoot "herdr.zip"
  $expanded = Join-Path $testRoot "herdr"
  Invoke-WebRequest -UseBasicParsing `
    "https://github.com/herdrdev/herdr/releases/download/v$herdrVersion/herdr-windows-x86_64.zip" `
    -OutFile $archive
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
  Assert-True ($hash -ceq $herdrSha256) "pinned Herdr $herdrVersion checksum differs: $hash"
  Expand-Archive -LiteralPath $archive -DestinationPath $expanded
  $found = Get-ChildItem -LiteralPath $expanded -Filter "herdr.exe" -File -Recurse | Select-Object -First 1
  Assert-True ($null -ne $found) "pinned Herdr archive contains no herdr.exe"
  $script:herdrExecutable = $found.FullName

  $env:XDG_CONFIG_HOME = Join-Path $testRoot "config"
  $env:XDG_STATE_HOME = Join-Path $testRoot "state"
  $env:HERDR_CONFIG_PATH = $null
  $env:HERDR_SESSION = "windowsfullpane"
  $env:HERDR_SOCKET_PATH = Join-Path $testRoot "server.sock"
  $env:HERDR_CLIENT_SOCKET_PATH = Join-Path $testRoot "client.sock"
  $env:PLANNOTATOR_TUI_BIN = $null
  $env:PLANNOTATOR_TUI_RELEASE_BASE = $null

  Start-Process -FilePath $script:herdrExecutable -ArgumentList "server" -WindowStyle Hidden
  $deadline = (Get-Date).AddSeconds(30)
  do {
    Start-Sleep -Milliseconds 500
    $status = Invoke-Herdr -Arguments @("status")
  } while ($status -notmatch "status: running" -and (Get-Date) -lt $deadline)
  Assert-True ($status -match "status: running") "the isolated Herdr server did not start"

  $created = Invoke-Herdr -Arguments @("workspace", "create", "--cwd", $review) | ConvertFrom-Json
  Assert-True ($created.result.type -ceq "workspace_created") "no isolated workspace was created"
  $originPane = $created.result.root_pane.pane_id

  $linked = Invoke-Herdr -Arguments @("plugin", "link", $variantRoot, "--enabled") | ConvertFrom-Json
  Assert-True ($linked.result.type -ceq "plugin_linked") "the isolated Herdr did not link Windows Full"
  Assert-True ($linked.result.plugin.plugin_id -ceq "annotate") "the linked plugin id is not annotate"

  # @() at every call site: an empty array returned from a function unrolls to $null.
  $before = @(Get-TuiProcessIds -Path $tui)
  Assert-True ($before.Count -eq 0) "a staged plannotator-tui was already running before the pane opened"

  $opened = Invoke-Herdr -Arguments @(
    "plugin", "pane", "open", "--plugin", "annotate", "--entrypoint", "doc", "--cwd", $review
  ) | ConvertFrom-Json
  Assert-True ($opened.result.type -ceq "plugin_pane_opened") "the doc pane did not open"
  $docPane = $opened.result.plugin_pane.pane.pane_id
  Assert-True ($docPane -cne $originPane) "the review pane and the originating pane share an id"

  # The render assertion: a per-run marker, read back out of the pane's own terminal.
  Invoke-Herdr -Arguments @("pane", "wait-output", $docPane, "--pattern", $marker, "--timeout", "30") | Out-Null
  $rendered = Invoke-Herdr -Arguments @("pane", "read", $docPane, "--source", "visible", "--format", "text")
  Assert-True ($rendered -match [regex]::Escape($marker)) `
    "the review pane never rendered $marker`n$rendered"
  Assert-True ($rendered -match "annotations") "the review pane rendered no plannotator-tui status line"
  Write-Output "doc pane rendered $marker from a review folder outside the checkout"

  $running = @(Get-TuiProcessIds -Path $tui)
  Assert-True ($running.Count -eq 1) "expected exactly one staged plannotator-tui, found $($running.Count)"

  Invoke-Herdr -Arguments @("pane", "send-keys", $docPane, "q") | Out-Null
  $deadline = (Get-Date).AddSeconds(20)
  do {
    Start-Sleep -Milliseconds 500
    $panes = Invoke-Herdr -Arguments @("pane", "list")
  } while ($panes -match [regex]::Escape($docPane) -and (Get-Date) -lt $deadline)
  Assert-True ($panes -notmatch [regex]::Escape($docPane)) "the review pane stayed open after q"
  Assert-True ($panes -match [regex]::Escape($originPane)) "the originating pane did not survive the review"

  $after = @(Get-TuiProcessIds -Path $tui)
  Assert-True ($after.Count -eq 0) "a staged plannotator-tui survived the review: $($after -join ',')"

  $log = Join-Path $env:XDG_CONFIG_HOME "herdr\sessions\$($env:HERDR_SESSION)\herdr-server.log"
  Assert-True (Test-Path -LiteralPath $log -PathType Leaf) "no isolated server log at $log"
  $exits = @(
    Get-Content -LiteralPath $log |
      Where-Object { $_ -match 'event="pane\.exit"' } |
      Select-Object -Last 1
  )
  Assert-True ($exits.Count -eq 1) "the isolated server logged no pane exit"
  Assert-True ($exits[0] -match "code: 0\b") "the review pane exited abnormally: $($exits[0])"
  Write-Output "q closed the review pane with status zero and left no process behind"
} finally {
  if ($null -ne $script:herdrExecutable) {
    & $script:herdrExecutable server stop *>&1 | Out-Null
    Start-Sleep -Seconds 2
  }
  foreach ($name in $isolatedNames) {
    if ($null -eq $oldEnvironment[$name]) {
      [Environment]::SetEnvironmentVariable($name, $null, "Process")
    } else {
      [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
    }
  }
  Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
