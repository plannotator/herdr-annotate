$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $env:RUNNER_TEMP ("pinned herdr plugin " + [guid]::NewGuid())
$pluginRoot = Join-Path $testRoot "plugin root with spaces"
$archive = Join-Path $testRoot "herdr-windows-x86_64.zip"
$expanded = Join-Path $testRoot "herdr"
$synthetic = Join-Path $testRoot "synthetic plannotator-tui.exe"
$oldEnvironment = @{}
foreach ($name in @(
  "PLANNOTATOR_TUI_BIN",
  "XDG_CONFIG_HOME",
  "XDG_STATE_HOME",
  "HERDR_CONFIG_PATH",
  "HERDR_SESSION",
  "HERDR_SOCKET_PATH",
  "HERDR_CLIENT_SOCKET_PATH"
)) {
  $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Invoke-Herdr {
  param([string]$Executable, [string[]]$Arguments)
  $output = & $Executable @Arguments *>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { throw "herdr $($Arguments -join ' ') failed: $output" }
  $output
}

try {
  New-Item -ItemType Directory -Force (Join-Path $pluginRoot "scripts") | Out-Null
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "herdr-plugin.toml") -Destination $pluginRoot
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "plannotator-tui.version") -Destination $pluginRoot
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts/fetch-plannotator-tui.ps1") `
    -Destination (Join-Path $pluginRoot "scripts")
  Set-Content -LiteralPath $synthetic -NoNewline -Value "synthetic executable"
  $env:PLANNOTATOR_TUI_BIN = $synthetic
  & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File (Join-Path $pluginRoot "scripts/fetch-plannotator-tui.ps1")
  if ($LASTEXITCODE -ne 0) { throw "local override failed before pinned-Herdr link" }
  Assert-True (
    (Test-Path -LiteralPath (Join-Path $pluginRoot "bin/plannotator-tui.exe") -PathType Leaf)
  ) "local override did not stage the Full executable"

  Invoke-WebRequest -UseBasicParsing `
    "https://github.com/herdrdev/herdr/releases/download/v0.8.2/herdr-windows-x86_64.zip" `
    -OutFile $archive
  $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
  Assert-True (
    $archiveHash -ceq "0ab3d0fe1434d55757997542b978c771d642987bb15a7130f4160f0db38821d5"
  ) "pinned Herdr archive checksum differs"
  Expand-Archive -LiteralPath $archive -DestinationPath $expanded
  $herdr = Get-ChildItem -LiteralPath $expanded -Filter "herdr.exe" -File -Recurse |
    Select-Object -First 1
  Assert-True ($null -ne $herdr) "pinned Herdr archive contains no herdr.exe"

  $env:PLANNOTATOR_TUI_BIN = $null
  $env:XDG_CONFIG_HOME = Join-Path $testRoot "isolated config"
  $env:XDG_STATE_HOME = Join-Path $testRoot "isolated state"
  $env:HERDR_CONFIG_PATH = $null
  $env:HERDR_SESSION = $null
  $env:HERDR_SOCKET_PATH = $null
  $env:HERDR_CLIENT_SOCKET_PATH = $null
  $link = Invoke-Herdr -Executable $herdr.FullName -Arguments @("plugin", "link", $pluginRoot, "--enabled")
  $linked = $link | ConvertFrom-Json
  Assert-True ($linked.result.type -ceq "plugin_linked") "pinned Herdr did not link the plugin"

  $listedText = Invoke-Herdr -Executable $herdr.FullName -Arguments @(
    "plugin", "list", "--plugin", "annotate", "--json"
  )
  $listed = $listedText | ConvertFrom-Json
  $plugins = @($listed.result.plugins)
  Assert-True ($plugins.Count -eq 1) "pinned Herdr did not list exactly one Annotate plugin"
  $plugin = $plugins[0]
  $actionIds = @($plugin.actions | ForEach-Object { $_.id })
  foreach ($id in @("capture", "copy-context", "manage", "open", "open-link", "last")) {
    Assert-True ($actionIds -contains $id) "pinned Herdr omitted action $id"
  }
  $paneIds = @($plugin.panes | ForEach-Object { $_.id })
  foreach ($id in @("editor", "manager", "doc")) {
    Assert-True ($paneIds -contains $id) "pinned Herdr omitted pane $id"
  }
  foreach ($id in @("open", "open-link", "last")) {
    $action = @($plugin.actions | Where-Object { $_.id -ceq $id })
    Assert-True ($action.Count -eq 1 -and @($action[0].platforms) -contains "windows") `
      "pinned Herdr did not retain the Windows gate for action $id"
  }
  $doc = @($plugin.panes | Where-Object { $_.id -ceq "doc" })
  Assert-True ($doc.Count -eq 1 -and @($doc[0].platforms) -contains "windows") `
    "pinned Herdr did not retain the Windows gate for pane doc"
} finally {
  foreach ($name in $oldEnvironment.Keys) {
    if ($null -eq $oldEnvironment[$name]) {
      [Environment]::SetEnvironmentVariable($name, $null, "Process")
    } else {
      [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
    }
  }
  Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
