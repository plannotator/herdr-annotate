param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("LocalOverride")]
  [string]$Case
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $env:RUNNER_TEMP ("herdr annotate fetch " + $Case + " " + [guid]::NewGuid())
$pluginRoot = Join-Path $testRoot "plugin root with spaces"
$pluginScripts = Join-Path $pluginRoot "scripts"
$fetcher = Join-Path $pluginScripts "fetch-herdr-annotate.ps1"
$destination = Join-Path $pluginRoot "bin/herdr-annotate.exe"
$stamp = Join-Path $pluginRoot "bin/herdr-annotate.version"
$version = (Get-Content -LiteralPath (Join-Path $repositoryRoot "herdr-annotate.version") -Raw).Trim()
$oldLocalOverride = [Environment]::GetEnvironmentVariable("HERDR_ANNOTATE_BIN", "Process")

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

try {
  New-Item -ItemType Directory -Force $pluginScripts | Out-Null
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts/fetch-herdr-annotate.ps1") -Destination $fetcher
  Copy-Item -LiteralPath (Join-Path $repositoryRoot "herdr-annotate.version") -Destination $pluginRoot

  $sourceDirectory = Join-Path $testRoot "synthetic source with spaces"
  $source = Join-Path $sourceDirectory "herdr-annotate local.exe"
  New-Item -ItemType Directory -Force $sourceDirectory | Out-Null
  Set-Content -LiteralPath $source -NoNewline -Value "local override bytes"
  New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
  Set-Content -LiteralPath $destination -NoNewline -Value "old destination bytes"
  Set-Content -LiteralPath $stamp -NoNewline -Value "old-version"

  $env:HERDR_ANNOTATE_BIN = $source
  $result = Invoke-Fetcher
  Assert-True ($result.ExitCode -eq 0) "local override failed: $($result.Output)"
  Assert-BytesEqual $source $destination "local override bytes differ"
  Assert-True ((Get-Content -LiteralPath $stamp -Raw) -ceq $version) "local stamp differs"

  $env:HERDR_ANNOTATE_BIN = $null
  $beforeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash
  $result = Invoke-Fetcher
  Assert-True ($result.ExitCode -eq 0) "idempotent run failed: $($result.Output)"
  Assert-True ($result.Output -match "already installed") "idempotent run did not short-circuit: $($result.Output)"
  Assert-True (
    (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
  ) "idempotent run replaced the destination"

  # A missing local build is fatal here: the annotation tools are this binary.
  $env:HERDR_ANNOTATE_BIN = Join-Path $testRoot "missing explicit override.exe"
  $result = Invoke-Fetcher
  Assert-True ($result.ExitCode -ne 0) "missing explicit override exited successfully"
  Assert-True ($result.Output -match "HERDR_ANNOTATE_BIN is not a file") "missing override error differs: $($result.Output)"
  Assert-True (
    (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
  ) "missing override changed the destination"

  $env:HERDR_ANNOTATE_BIN = $null
  Set-Content -LiteralPath (Join-Path $pluginRoot "herdr-annotate.version") -NoNewline -Value ""
  $result = Invoke-Fetcher
  Assert-True ($result.ExitCode -ne 0) "empty version pin exited successfully"
  Assert-True ($result.Output -match "herdr-annotate.version is empty") "empty pin error differs: $($result.Output)"
  Assert-True (
    (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ceq $beforeHash
  ) "empty pin changed the destination"
} finally {
  if ($null -eq $oldLocalOverride) {
    $env:HERDR_ANNOTATE_BIN = $null
  } else {
    $env:HERDR_ANNOTATE_BIN = $oldLocalOverride
  }
  Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
