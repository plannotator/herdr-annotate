$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$version = [string](Get-Content -LiteralPath "plannotator-tui.version" -Raw)
$version = $version.Trim()
if (-not $version) { throw "plannotator-tui.version is empty" }

$destinationDirectory = "bin"
$destination = Join-Path $destinationDirectory "plannotator-tui.exe"
$stamp = Join-Path $destinationDirectory "plannotator-tui.version"
New-Item -ItemType Directory -Force $destinationDirectory | Out-Null

$localOverride = [Environment]::GetEnvironmentVariable("PLANNOTATOR_TUI_BIN", "Process")
$hasLocalOverride = $null -ne $localOverride
$installed = if (Test-Path -LiteralPath $stamp -PathType Leaf) {
  ([string](Get-Content -LiteralPath $stamp -Raw)).Trim()
} else {
  ""
}

if ((Test-Path -LiteralPath $destination -PathType Leaf) -and
    $installed -eq $version -and -not $hasLocalOverride) {
  Write-Output "plannotator-tui $version already installed"
  exit 0
}

function Install-PlannotatorTui {
  param([Parameter(Mandatory = $true)][string]$Source)

  $candidate = Join-Path $destinationDirectory ("plannotator-tui-" + [guid]::NewGuid() + ".tmp")
  try {
    Copy-Item -LiteralPath $Source -Destination $candidate
    if (Test-Path -LiteralPath $destination -PathType Leaf) {
      try {
        [System.IO.File]::Replace(
          [System.IO.Path]::GetFullPath($candidate),
          [System.IO.Path]::GetFullPath($destination),
          $null
        )
      } catch {
        throw "failed to replace ${destination}: $($_.Exception.Message)"
      }
    } else {
      Move-Item -LiteralPath $candidate -Destination $destination
    }
    Set-Content -LiteralPath $stamp -NoNewline -Value $version
  } finally {
    Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
  }
}

if ($hasLocalOverride) {
  if (-not (Test-Path -LiteralPath $localOverride -PathType Leaf)) {
    throw "PLANNOTATOR_TUI_BIN is not a file: $localOverride"
  }
  Install-PlannotatorTui -Source $localOverride
  Write-Output "installed plannotator-tui from $localOverride (local build, stamped $version)"
  exit 0
}

try {
  $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  $target = switch ($architecture) {
    "X64" { "x86_64-pc-windows-msvc" }
    "Arm64" { "aarch64-pc-windows-msvc" }
    default { throw "no plannotator-tui release target for Windows/$architecture" }
  }

  $asset = "plannotator-tui-$target.exe"
  $releaseBaseOverride = [Environment]::GetEnvironmentVariable(
    "PLANNOTATOR_TUI_RELEASE_BASE",
    "Process"
  )
  # PLANNOTATOR_TUI_RELEASE_BASE is a test-only seam for a loopback fixture server.
  $base = if ($null -ne $releaseBaseOverride) {
    $releaseBaseOverride.TrimEnd([char]"/")
  } else {
    "https://github.com/plannotator/plannotator-tui/releases/download/v$version"
  }

  $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("plannotator-tui-" + [guid]::NewGuid())
  try {
    New-Item -ItemType Directory $temporary | Out-Null
    $downloadedAsset = Join-Path $temporary $asset
    $checksumFile = Join-Path $temporary "SHA256SUMS"
    Invoke-WebRequest -UseBasicParsing "$base/$asset" -OutFile $downloadedAsset
    Invoke-WebRequest -UseBasicParsing "$base/SHA256SUMS" -OutFile $checksumFile

    $matches = @(
      Get-Content -LiteralPath $checksumFile | Where-Object {
        $fields = @($_ -split "\s+")
        $fields.Count -ge 2 -and $fields[-1] -ceq $asset
      }
    )
    if ($matches.Count -ne 1) {
      throw "expected exactly one checksum for $asset in $base/SHA256SUMS; found $($matches.Count)"
    }
    $checksumFields = $matches[0].Trim() -split "\s+"
    $expected = $checksumFields[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $downloadedAsset).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
      throw "sha256 mismatch for ${asset}: expected $expected, got $actual"
    }

    Install-PlannotatorTui -Source $downloadedAsset
    Write-Output "installed plannotator-tui $version ($target)"
  } finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
  }
} catch {
  Write-Warning (
    "Full review is unavailable until the plugin is reinstalled or updated: " +
    $_.Exception.Message
  )
  exit 0
}
