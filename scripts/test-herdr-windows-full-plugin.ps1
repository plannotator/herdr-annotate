# Windows Full against pinned Herdr releases, in an isolated config/state/socket so the
# machine's own Herdr install and session are never touched.
#
# Two things are proved here that the manifest test cannot see, because they are Herdr's
# behaviour rather than the file's contents: 0.8.2 refuses the variant for its minimum, and
# 0.9.0 accepts it and reports every action, pane and link handler as effective on Windows.
# The plugin root contains spaces on purpose.
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
# RUNNER_TEMP is the CI location; falling back to TEMP lets a human run this unchanged when
# recording the native qualification matrix.
$temporaryBase = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { $env:TEMP } else { $env:RUNNER_TEMP }
$testRoot = Join-Path $temporaryBase ("herdr windows full " + [guid]::NewGuid())
$pluginRoot = Join-Path $testRoot "plugin root with spaces"

$releases = @(
  @{
    Version  = "0.8.2"
    Sha256   = "0ab3d0fe1434d55757997542b978c771d642987bb15a7130f4160f0db38821d5"
    Accepts  = $false
  }
  @{
    Version  = "0.9.0"
    Sha256   = "b4508c445de1c1a68c760a01735da2aba2fa214b2aafd4b07f732e49b2a64b11"
    Accepts  = $true
  }
)

$isolatedNames = @(
  "XDG_CONFIG_HOME",
  "XDG_STATE_HOME",
  "HERDR_CONFIG_PATH",
  "HERDR_SESSION",
  "HERDR_SOCKET_PATH",
  "HERDR_CLIENT_SOCKET_PATH"
)
$oldEnvironment = @{}
foreach ($name in $isolatedNames) {
  $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Get-Platforms {
  # An entry with no gate carries no `platforms` key at all, and Herdr omits it from the
  # JSON rather than echoing the manifest default. Under StrictMode that is a missing
  # property, not an empty one, so it is read through PSObject and reported as "no gate".
  param($Item)
  $property = $Item.PSObject.Properties['platforms']
  if ($null -eq $property -or $null -eq $property.Value) { return @() }
  @($property.Value)
}

function Get-Herdr {
  param([string]$Version, [string]$Sha256, [string]$Destination)
  $archive = Join-Path $Destination "herdr-$Version.zip"
  $expanded = Join-Path $Destination "herdr-$Version"
  Invoke-WebRequest -UseBasicParsing `
    "https://github.com/herdrdev/herdr/releases/download/v$Version/herdr-windows-x86_64.zip" `
    -OutFile $archive
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
  Assert-True ($hash -ceq $Sha256) "pinned Herdr $Version archive checksum differs: $hash"
  Expand-Archive -LiteralPath $archive -DestinationPath $expanded
  $executable = Get-ChildItem -LiteralPath $expanded -Filter "herdr.exe" -File -Recurse |
    Select-Object -First 1
  Assert-True ($null -ne $executable) "pinned Herdr $Version archive contains no herdr.exe"
  $executable.FullName
}

try {
  New-Item -ItemType Directory -Force $pluginRoot | Out-Null
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "windows-full\herdr-plugin.toml") `
    -Destination $pluginRoot

  foreach ($release in $releases) {
    $version = $release.Version
    $herdr = Get-Herdr -Version $version -Sha256 $release.Sha256 -Destination $testRoot

    # A separate config, state and socket per release: the point is that nothing here can
    # reach the machine's own server, and that the two releases cannot reach each other's.
    $env:XDG_CONFIG_HOME = Join-Path $testRoot "config $version"
    $env:XDG_STATE_HOME = Join-Path $testRoot "state $version"
    $env:HERDR_CONFIG_PATH = $null
    $env:HERDR_SESSION = $null
    $env:HERDR_SOCKET_PATH = Join-Path $testRoot "server-$version.sock"
    $env:HERDR_CLIENT_SOCKET_PATH = Join-Path $testRoot "client-$version.sock"

    $output = & $herdr plugin link $pluginRoot --enabled *>&1 | Out-String

    if (-not $release.Accepts) {
      # The acceptance checklist names the code `plugin_requires_newer_herdr`. What 0.8.2's
      # `plugin link` actually prints is an unstructured message, so both spellings are
      # accepted and the observed one is echoed rather than hidden behind a pass.
      Assert-True (
        $output -match "plugin_requires_newer_herdr" -or
        $output -match "requires Herdr 0\.9\.0 or newer"
      ) "Herdr $version did not reject Windows Full for its minimum: $output"
      Assert-True ($output -notmatch "plugin_linked") "Herdr $version linked Windows Full anyway"
      Write-Output "herdr $version rejects windows-full: $($output.Trim())"
      continue
    }

    $linked = $output | ConvertFrom-Json
    Assert-True ($linked.result.type -ceq "plugin_linked") `
      "Herdr $version did not link Windows Full: $output"

    $listedText = & $herdr plugin list --plugin annotate --json *>&1 | Out-String
    $listed = $listedText | ConvertFrom-Json
    $plugins = @($listed.result.plugins)
    Assert-True ($plugins.Count -eq 1) "Herdr $version did not list exactly one Annotate plugin"
    $plugin = $plugins[0]

    # Every entry must be effective on Windows. An inherited Unix gate would leave the
    # variant installed and the review half silently unreachable, which is the state this
    # whole variant exists to end.
    $actionIds = @($plugin.actions | ForEach-Object { $_.id })
    foreach ($id in @("capture", "copy-context", "copy-archive", "paste-archive", "send-archive", "manage", "open", "open-link", "last", "last-newest", "terminal")) {
      Assert-True ($actionIds -contains $id) "Herdr $version omitted action $id"
      $action = @($plugin.actions | Where-Object { $_.id -ceq $id })
      # @() at the call site: an empty array returned from a function unrolls to $null.
      $platforms = @(Get-Platforms -Item $action[0])
      Assert-True ($platforms.Count -eq 0 -or $platforms -contains "windows") `
        "action $id is not effective on Windows: $($platforms -join ',')"
    }
    Assert-True ($actionIds.Count -eq 11) "Herdr $version listed $($actionIds.Count) actions, expected 11"

    $paneIds = @($plugin.panes | ForEach-Object { $_.id })
    foreach ($id in @("editor", "manager", "doc")) {
      Assert-True ($paneIds -contains $id) "Herdr $version omitted pane $id"
    }
    Assert-True ($paneIds.Count -eq 3) "Herdr $version listed $($paneIds.Count) panes, expected 3"

    $doc = @($plugin.panes | Where-Object { $_.id -ceq "doc" })[0]
    $docPlatforms = @(Get-Platforms -Item $doc)
    Assert-True ($docPlatforms.Count -eq 0 -or $docPlatforms -contains "windows") `
      "the doc pane is not effective on Windows"
    # No launcher may sit between Herdr and the TUI: a process started through the
    # extended-length path Herdr resolves to exits immediately unless it is native.
    $docCommand = @($doc.command)
    Assert-True (
      $docCommand.Count -eq 3 -and
      $docCommand[0] -ceq "./bin/plannotator-tui.exe" -and
      $docCommand[1] -ceq "herdr" -and
      $docCommand[2] -ceq "pane"
    ) "the doc pane is not direct argv: $($docCommand -join ' ')"

    $handler = @($plugin.link_handlers | Where-Object { $_.id -ceq "markdown-file" })
    Assert-True ($handler.Count -eq 1) "Herdr $version omitted the markdown-file link handler"

    Write-Output "herdr $version accepts windows-full: 11 actions, 3 panes, direct-argv doc pane"
  }
} finally {
  foreach ($name in $isolatedNames) {
    if ($null -eq $oldEnvironment[$name]) {
      [Environment]::SetEnvironmentVariable($name, $null, "Process")
    } else {
      [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
    }
  }
  Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
