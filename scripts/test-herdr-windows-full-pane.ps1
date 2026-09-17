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
#
# Run with no arguments for the default case. -Checkout and -Review let the path-matrix suite
# reuse this lifecycle against awkward roots without restating it; caller-supplied directories
# are left in place on exit, since the caller owns them. -HerdrExecutable skips the download
# when a suite has already fetched and verified one.
param(
  [string]$Checkout,
  [string]$Review,
  [string]$HerdrExecutable,
  [string]$LinkPath,
  [string]$Label = "default"
)

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
$callerOwnsDirectories = -not [string]::IsNullOrWhiteSpace($Checkout)
$checkout = if ($callerOwnsDirectories) { $Checkout } else { Join-Path $testRoot "checkout with spaces" }
$variantRoot = Join-Path $checkout "windows-full"
$review = if (-not [string]::IsNullOrWhiteSpace($Review)) { $Review } else { Join-Path $testRoot "review folder" }
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

# Distinct from the -HerdrExecutable parameter: PowerShell variable names are
# case-insensitive, so reusing that spelling here would blank the parameter.
$resolvedHerdr = $null

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Invoke-Herdr {
  param([string[]]$Arguments)
  $output = & $script:resolvedHerdr @Arguments *>&1 | Out-String
  $output
}

function Get-TuiProcessPaths {
  # Scoped to the staged executable so a developer's own plannotator-tui is never counted.
  # Compared through GetFullPath and case-insensitively, because Windows reports a process
  # image under a UNC root in its own spelling rather than the one used to launch it.
  param([string]$Path)

  # Windows reports the image of a process launched from a share in device form, where the
  # \\server\share prefix appears as UNC\server\share. Folded back before comparing, or every
  # UNC run looks like no process at all.
  function Resolve-ImagePath {
    param([string]$Candidate)
    $value = if ($Candidate -like "UNC\*") { "\\" + $Candidate.Substring(4) } else { $Candidate }
    try { [System.IO.Path]::GetFullPath($value) } catch { $value }
  }

  $wanted = Resolve-ImagePath -Candidate $Path
  @(
    Get-Process plannotator-tui -ErrorAction SilentlyContinue |
      ForEach-Object {
        $candidate = $null
        try { $candidate = $_.Path } catch { $candidate = $null }
        if ($null -ne $candidate) {
          $full = Resolve-ImagePath -Candidate $candidate
          if ($full -ieq $wanted) { $full }
        }
      }
  )
}

try {
  # .NET rather than New-Item throughout: these paths legitimately contain brackets, which
  # PowerShell's -Path parameters treat as wildcards. Only -LiteralPath and the .NET APIs
  # read them as the characters they are.
  [System.IO.Directory]::CreateDirectory($review) | Out-Null
  [System.IO.Directory]::CreateDirectory((Join-Path $variantRoot "scripts")) | Out-Null
  [System.IO.Directory]::CreateDirectory((Join-Path $checkout "scripts")) | Out-Null
  Set-Content -LiteralPath (Join-Path $review $fixture) -Encoding utf8 -Value @(
    "# $marker",
    "",
    "Unique fixture content for this run: $marker"
  )

  # A staged checkout rather than the working tree: the build must work from a relocated
  # copy, which is what an installed plugin is.
  foreach ($name in @("plannotator-tui.version", "herdr-annotate.version")) {
    [System.IO.File]::Copy((Join-Path $repositoryRoot $name), (Join-Path $checkout $name), $true)
  }
  foreach ($pair in @(
    @("scripts\fetch-plannotator-tui.ps1", (Join-Path $checkout "scripts\fetch-plannotator-tui.ps1")),
    @("scripts\fetch-herdr-annotate.ps1", (Join-Path $checkout "scripts\fetch-herdr-annotate.ps1")),
    @("windows-full\herdr-plugin.toml", (Join-Path $variantRoot "herdr-plugin.toml")),
    @("windows-full\scripts\fetch-plannotator-tui.ps1", (Join-Path $variantRoot "scripts\fetch-plannotator-tui.ps1"))
  )) {
    [System.IO.File]::Copy((Join-Path $repositoryRoot $pair[0]), $pair[1], $true)
  }

  # Both build entries from the manifest, run the way Herdr runs them: from the plugin root,
  # with no override variables set, against the real release. The overrides are cleared here
  # rather than with the rest of the isolation below, because the production fetch is the one
  # thing that must not see them, and it happens first. $env: assignment rather than
  # SetEnvironmentVariable: passing $null to the latter binds as an empty string and leaves
  # the name defined, which a fetcher testing presence rather than emptiness then reads as a
  # release base. The $env: form removes the name outright.
  $env:PLANNOTATOR_TUI_BIN = $null
  $env:PLANNOTATOR_TUI_RELEASE_BASE = $null
  $env:HERDR_ANNOTATE_BIN = $null

  Push-Location -LiteralPath $variantRoot
  try {
    # Kept, not discarded: both fetchers warn and exit zero by design so Lite survives a
    # failed download, which means a silent staging failure is the normal shape of trouble
    # here and the text is the only evidence of why.
    $buildLog = @()
    $buildLog += (& powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass `
      -File "..\scripts\fetch-herdr-annotate.ps1" *>&1 | Out-String)
    $buildLog += (& powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass `
      -File "scripts\fetch-plannotator-tui.ps1" *>&1 | Out-String)
    $script:buildOutput = ($buildLog -join "").Trim()
  } finally {
    Pop-Location
  }

  $tui = Join-Path $variantRoot "bin\plannotator-tui.exe"
  $native = Join-Path $checkout "bin\herdr-annotate.exe"
  Assert-True (Test-Path -LiteralPath $tui -PathType Leaf) `
    "the variant build staged no plannotator-tui.exe`n--- build output ---`n$script:buildOutput"
  Assert-True (Test-Path -LiteralPath $native -PathType Leaf) `
    "the native build staged no herdr-annotate.exe`n--- build output ---`n$script:buildOutput"
  $pin = (Get-Content -LiteralPath (Join-Path $checkout "plannotator-tui.version") -Raw).Trim()
  $stamp = (Get-Content -LiteralPath (Join-Path $variantRoot "bin\plannotator-tui.version") -Raw).Trim()
  Assert-True ($stamp -ceq $pin) "staged stamp $stamp does not match the pin $pin"
  # Process creation takes the image path through the ANSI/MAX_PATH route and accepts no
  # extended-length spelling, so past 260 characters a staged, verified, present binary still
  # cannot be started. Reported as that, rather than as a stack trace about --version.
  $reported = try {
    (& $tui --version *>&1 | Out-String).Trim()
  } catch {
    throw (
      "the staged TUI cannot be started: its path is $($tui.Length) characters" +
      $(if ($tui.Length -gt 260) { ", past the $([int]260)-character process-creation limit" } else { "" }) +
      ". Windows reported: $($_.Exception.Message.Split([char]10)[0].Trim())"
    )
  }
  Assert-True ($reported -match [regex]::Escape($pin)) "staged TUI reports '$reported', expected $pin"
  Write-Output "[$Label] staged plannotator-tui $pin at windows-full/bin, native runtime one level up"

  if ([string]::IsNullOrWhiteSpace($HerdrExecutable)) {
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
    $script:resolvedHerdr = $found.FullName
  } else {
    Assert-True (Test-Path -LiteralPath $HerdrExecutable -PathType Leaf) `
      "no Herdr executable at $HerdrExecutable"
    $script:resolvedHerdr = $HerdrExecutable
  }

  $env:XDG_CONFIG_HOME = Join-Path $testRoot "config"
  $env:XDG_STATE_HOME = Join-Path $testRoot "state"
  $env:HERDR_CONFIG_PATH = $null
  $env:HERDR_SESSION = "windowsfullpane"
  $env:HERDR_SOCKET_PATH = Join-Path $testRoot "server.sock"
  $env:HERDR_CLIENT_SOCKET_PATH = Join-Path $testRoot "client.sock"
  $env:PLANNOTATOR_TUI_BIN = $null
  $env:PLANNOTATOR_TUI_RELEASE_BASE = $null

  Start-Process -FilePath $script:resolvedHerdr -ArgumentList "server" -WindowStyle Hidden
  $deadline = (Get-Date).AddSeconds(30)
  do {
    Start-Sleep -Milliseconds 500
    $status = Invoke-Herdr -Arguments @("status")
  } while ($status -notmatch "status: running" -and (Get-Date) -lt $deadline)
  Assert-True ($status -match "status: running") "the isolated Herdr server did not start"

  $created = Invoke-Herdr -Arguments @("workspace", "create", "--cwd", $review) | ConvertFrom-Json
  Assert-True ($created.result.type -ceq "workspace_created") "no isolated workspace was created"
  $originPane = $created.result.root_pane.pane_id

  # -LinkPath lets a caller hand Herdr a different spelling of the same directory -- an
  # extended-length \\?\ root, say -- while staging still happens through the ordinary path,
  # which is what Herdr's own build step does. Herdr must resolve it to the same directory.
  $linkTarget = if ([string]::IsNullOrWhiteSpace($LinkPath)) { $variantRoot } else { $LinkPath }
  $linked = Invoke-Herdr -Arguments @("plugin", "link", $linkTarget, "--enabled") | ConvertFrom-Json
  Assert-True ($linked.result.type -ceq "plugin_linked") "the isolated Herdr did not link Windows Full"
  Assert-True ($linked.result.plugin.plugin_id -ceq "annotate") "the linked plugin id is not annotate"
  $reportedRoot = [string]$linked.result.plugin.plugin_root
  Assert-True (
    [System.IO.Path]::GetFullPath($reportedRoot.Replace("\\?\", "")) -ceq
    [System.IO.Path]::GetFullPath($variantRoot.Replace("\\?\", ""))
  ) "Herdr reported plugin root '$reportedRoot', which is not the staged directory"

  # @() at every call site: an empty array returned from a function unrolls to $null.
  $before = @(Get-TuiProcessPaths -Path $tui)
  Assert-True ($before.Count -eq 0) "a staged plannotator-tui was already running before the pane opened"

  $opened = Invoke-Herdr -Arguments @(
    "plugin", "pane", "open", "--plugin", "annotate", "--entrypoint", "doc", "--cwd", $review
  ) | ConvertFrom-Json
  Assert-True ($opened.result.type -ceq "plugin_pane_opened") "the doc pane did not open"
  $docPane = $opened.result.plugin_pane.pane.pane_id
  Assert-True ($docPane -cne $originPane) "the review pane and the originating pane share an id"

  # The render assertion: a per-run marker, read back out of the pane's own terminal.
  # --match, not --pattern, and --timeout is milliseconds. Getting either wrong makes the call
  # fail instead of waiting, and the assertion below then races the TUI's first paint -- which
  # is why its exit status is checked rather than discarded.
  $waited = Invoke-Herdr -Arguments @(
    "pane", "wait-output", $docPane, "--match", $marker, "--timeout", "30000"
  )
  Assert-True ($LASTEXITCODE -eq 0) "waiting for $marker in the review pane failed: $waited"
  $rendered = Invoke-Herdr -Arguments @("pane", "read", $docPane, "--source", "visible", "--format", "text")
  Assert-True ($rendered -match [regex]::Escape($marker)) `
    "the review pane never rendered $marker`n$rendered"
  Assert-True ($rendered -match "annotations") "the review pane rendered no plannotator-tui status line"
  Write-Output "[$Label] doc pane rendered $marker; plugin root $($variantRoot.Length) chars, review $($review.Length)"

  $running = @(Get-TuiProcessPaths -Path $tui)
  $seen = @(Get-Process plannotator-tui -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Path } catch { "<unreadable>" }
  })
  Assert-True ($running.Count -eq 1) (
    "expected exactly one staged plannotator-tui at`n  $tui`nfound $($running.Count). " +
    "Running images: $($seen -join '; ')"
  )

  Invoke-Herdr -Arguments @("pane", "send-keys", $docPane, "q") | Out-Null
  $deadline = (Get-Date).AddSeconds(20)
  do {
    Start-Sleep -Milliseconds 500
    $panes = Invoke-Herdr -Arguments @("pane", "list")
  } while ($panes -match [regex]::Escape($docPane) -and (Get-Date) -lt $deadline)
  Assert-True ($panes -notmatch [regex]::Escape($docPane)) "the review pane stayed open after q"
  Assert-True ($panes -match [regex]::Escape($originPane)) "the originating pane did not survive the review"

  $after = @(Get-TuiProcessPaths -Path $tui)
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
  Write-Output "[$Label] q closed the review pane with status zero and left no process behind"
} finally {
  if ($null -ne $script:resolvedHerdr) {
    & $script:resolvedHerdr server stop *>&1 | Out-Null
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
