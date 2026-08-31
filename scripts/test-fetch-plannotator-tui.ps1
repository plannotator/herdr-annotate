param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("LocalOverride", "Download")]
  [string]$Case
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $env:RUNNER_TEMP ("plannotator full fetch " + $Case + " " + [guid]::NewGuid())
$pluginRoot = Join-Path $testRoot "plugin root with spaces"
$pluginScripts = Join-Path $pluginRoot "scripts"
$fetcher = Join-Path $pluginScripts "fetch-plannotator-tui.ps1"
$destination = Join-Path $pluginRoot "bin/plannotator-tui.exe"
$stamp = Join-Path $pluginRoot "bin/plannotator-tui.version"
$oldLocalOverride = [Environment]::GetEnvironmentVariable("PLANNOTATOR_TUI_BIN", "Process")
$oldReleaseBase = [Environment]::GetEnvironmentVariable("PLANNOTATOR_TUI_RELEASE_BASE", "Process")

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Invoke-Fetcher {
  $output = & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $fetcher *>&1 |
    Out-String
  [pscustomobject]@{ ExitCode = $LASTEXITCODE; Output = $output }
}

function Assert-BytesEqual {
  param([string]$Left, [string]$Right, [string]$Message)
  $leftBytes = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($Left))
  $rightBytes = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($Right))
  Assert-True ($leftBytes -ceq $rightBytes) $Message
}

function Start-FixtureServer {
  param([string]$Root, [string]$PortFile)
  $start = [System.Diagnostics.ProcessStartInfo]::new()
  $start.FileName = (Get-Command python).Source
  $start.UseShellExecute = $false
  $start.ArgumentList.Add((Join-Path $repositoryRoot "scripts/test-http-server.py"))
  $start.ArgumentList.Add($Root)
  $start.ArgumentList.Add($PortFile)
  $process = [System.Diagnostics.Process]::Start($start)
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while (-not (Test-Path -LiteralPath $PortFile -PathType Leaf)) {
    if ($process.HasExited) { throw "fixture server exited with $($process.ExitCode)" }
    if ([DateTime]::UtcNow -gt $deadline) { throw "fixture server did not publish its port" }
    Start-Sleep -Milliseconds 100
  }
  $process
}

try {
  New-Item -ItemType Directory -Force $pluginScripts | Out-Null
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts/fetch-plannotator-tui.ps1") -Destination $fetcher
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "plannotator-tui.version") -Destination $pluginRoot

  if ($Case -eq "LocalOverride") {
    $sourceDirectory = Join-Path $testRoot "synthetic source with spaces"
    $source = Join-Path $sourceDirectory "plannotator-tui local.exe"
    New-Item -ItemType Directory -Force $sourceDirectory | Out-Null
    Set-Content -LiteralPath $source -NoNewline -Value "local override bytes"
    New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
    Set-Content -LiteralPath $destination -NoNewline -Value "old destination bytes"
    Set-Content -LiteralPath $stamp -NoNewline -Value "old-version"

    $env:PLANNOTATOR_TUI_BIN = $source
    $result = Invoke-Fetcher
    Assert-True ($result.ExitCode -eq 0) "local override failed: $($result.Output)"
    Assert-BytesEqual $source $destination "local override bytes differ"
    Assert-True ((Get-Content -LiteralPath $stamp -Raw) -ceq "0.6.0") "local stamp differs"

    $env:PLANNOTATOR_TUI_BIN = $null
    $env:PLANNOTATOR_TUI_RELEASE_BASE = "http://127.0.0.1:1/must-not-be-requested"
    $beforeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash
    $result = Invoke-Fetcher
    Assert-True ($result.ExitCode -eq 0) "idempotent run failed: $($result.Output)"
    Assert-True ($result.Output -match "already installed") "idempotent run did not short-circuit"
    Assert-True (
      (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
    ) "idempotent run replaced the destination"

    $env:PLANNOTATOR_TUI_BIN = Join-Path $testRoot "missing explicit override.exe"
    $result = Invoke-Fetcher
    Assert-True ($result.ExitCode -ne 0) "missing explicit override exited successfully"
    Assert-True ($result.Output -match "PLANNOTATOR_TUI_BIN is not a file") "missing override error differs"
    Assert-True (
      (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
    ) "missing override changed the destination"

    $env:PLANNOTATOR_TUI_BIN = $null
    Set-Content -LiteralPath (Join-Path $pluginRoot "plannotator-tui.version") -NoNewline -Value ""
    $result = Invoke-Fetcher
    Assert-True ($result.ExitCode -ne 0) "empty version pin exited successfully"
    Assert-True ($result.Output -match "plannotator-tui.version is empty") "empty pin error differs"
    Assert-True (
      (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
    ) "empty pin changed the destination"
  } else {
    $webRoot = Join-Path $testRoot "loopback release with spaces"
    $portFile = Join-Path $testRoot "fixture-server.port"
    $asset = "plannotator-tui-x86_64-pc-windows-msvc.exe"
    $source = Join-Path $webRoot $asset
    New-Item -ItemType Directory -Force $webRoot | Out-Null
    Set-Content -LiteralPath $source -NoNewline -Value "downloaded fixture bytes"
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $webRoot "SHA256SUMS") -Value "$hash  $asset"
    $server = Start-FixtureServer -Root $webRoot -PortFile $portFile
    try {
      $port = (Get-Content -LiteralPath $portFile -Raw).Trim()
      $env:PLANNOTATOR_TUI_BIN = $null
      $env:PLANNOTATOR_TUI_RELEASE_BASE = "http://127.0.0.1:$port"
      $result = Invoke-Fetcher
      Assert-True ($result.ExitCode -eq 0) "download fixture failed: $($result.Output)"
      Assert-True ($result.Output -match "x86_64-pc-windows-msvc") "x64 target was not selected"
      Assert-BytesEqual $source $destination "downloaded destination bytes differ"
      Assert-True ((Get-Content -LiteralPath $stamp -Raw) -ceq "0.6.0") "download stamp differs"

      Set-Content -LiteralPath $stamp -NoNewline -Value "preserve-this-stamp"
      $beforeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash
      Set-Content -LiteralPath (Join-Path $webRoot "SHA256SUMS") -Value ("0" * 64 + "  $asset")
      $result = Invoke-Fetcher
      Assert-True ($result.ExitCode -eq 0) "wrong checksum was fatal: $($result.Output)"
      Assert-True ($result.Output -match "Full review is unavailable") "wrong checksum warning differs"
      Assert-True (
        (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
      ) "wrong checksum changed the prior destination"
      Assert-True (
        (Get-Content -LiteralPath $stamp -Raw) -ceq "preserve-this-stamp"
      ) "wrong checksum changed the prior stamp"

      Set-Content -LiteralPath $source -NoNewline -Value "replacement bytes while destination is locked"
      $replacementHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant()
      Set-Content -LiteralPath (Join-Path $webRoot "SHA256SUMS") `
        -Value "$replacementHash  $asset"
      Set-Content -LiteralPath $stamp -NoNewline -Value "preserve-locked-stamp"
      $beforeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash
      $lock = [System.IO.File]::Open(
        $destination,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read
      )
      try {
        $result = Invoke-Fetcher
      } finally {
        $lock.Dispose()
      }
      Assert-True ($result.ExitCode -eq 0) "locked destination was fatal: $($result.Output)"
      Assert-True ($result.Output -match "Full review is unavailable") "locked warning differs"
      Assert-True ($result.Output -match "plannotator-tui.exe") "locked warning omits destination"
      Assert-True (
        (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
      ) "locked replacement changed the prior destination"
      Assert-True (
        (Get-Content -LiteralPath $stamp -Raw) -ceq "preserve-locked-stamp"
      ) "locked replacement changed the prior stamp"
    } finally {
      if ($null -ne $server -and -not $server.HasExited) {
        $server.Kill($true)
        $server.WaitForExit()
      }
    }
  }
} finally {
  if ($null -eq $oldLocalOverride) {
    $env:PLANNOTATOR_TUI_BIN = $null
  } else {
    $env:PLANNOTATOR_TUI_BIN = $oldLocalOverride
  }
  if ($null -eq $oldReleaseBase) {
    $env:PLANNOTATOR_TUI_RELEASE_BASE = $null
  } else {
    $env:PLANNOTATOR_TUI_RELEASE_BASE = $oldReleaseBase
  }
  Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
