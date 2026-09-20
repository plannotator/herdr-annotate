$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$versionContents = Get-Content -LiteralPath "plannotator-tui.version" -Raw
$version = if ($null -eq $versionContents) { "" } else { [string]$versionContents }
$version = $version.Trim()
if (-not $version) { throw "plannotator-tui.version is empty" }

$destinationDirectory = Join-Path (Get-Location).Path "bin"
$destination = Join-Path $destinationDirectory "plannotator-tui.exe"
$stamp = Join-Path $destinationDirectory "plannotator-tui.version"
$targetFile = Join-Path $destinationDirectory "plannotator-tui.target"
New-Item -ItemType Directory -Force $destinationDirectory | Out-Null

$localOverride = [Environment]::GetEnvironmentVariable("PLANNOTATOR_TUI_BIN", "Process")
$hasLocalOverride = $null -ne $localOverride
$installed = if (Test-Path -LiteralPath $stamp -PathType Leaf) {
  ([string](Get-Content -LiteralPath $stamp -Raw)).Trim()
} else {
  ""
}
$installedTarget = if (Test-Path -LiteralPath $targetFile -PathType Leaf) {
  ([string](Get-Content -LiteralPath $targetFile -Raw)).Trim()
} else {
  ""
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$target = switch ($architecture) {
  "X64" { "x86_64-pc-windows-msvc" }
  "Arm64" { "aarch64-pc-windows-msvc" }
  default { "unknown" }
}
if ($target -eq "unknown") {
  if (Get-Command "node" -ErrorAction SilentlyContinue) {
    try {
      $nodeArch = (& node -e "console.log(process.arch)").Trim()
      if ($nodeArch -eq "arm64") { $target = "aarch64-pc-windows-msvc" }
      elseif ($nodeArch -eq "x64") { $target = "x86_64-pc-windows-msvc" }
    } catch {}
  }
}

function Test-RunnableAndMatches {
  if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) { return $false }
  if ($installedTarget -and $target -ne "unknown" -and $installedTarget -ne $target) {
    return $false
  }
  return $true
}

if ((Test-Path -LiteralPath $destination -PathType Leaf) -and
    $installed -eq $version -and (Test-RunnableAndMatches) -and -not $hasLocalOverride) {
  if (-not (Test-Path -LiteralPath $targetFile -PathType Leaf) -and $target -ne "unknown") {
    Set-Content -LiteralPath $targetFile -NoNewline -Value $target
  }
  Write-Output "plannotator-tui $version already installed"
  exit 0
}

function Install-PlannotatorTui {
  param([Parameter(Mandatory = $true)][string]$Source)

  $candidate = Join-Path $destinationDirectory ("plannotator-tui-" + [guid]::NewGuid() + ".tmp")
  $backup = Join-Path $destinationDirectory ("plannotator-tui-" + [guid]::NewGuid() + ".bak")
  $stampBackup = Join-Path $destinationDirectory ("plannotator-tui-version-" + [guid]::NewGuid() + ".bak")
  $hadDestination = Test-Path -LiteralPath $destination -PathType Leaf
  $hadStamp = Test-Path -LiteralPath $stamp -PathType Leaf
  $replacementCompleted = $false
  $keepRecoveryFiles = $false
  try {
    if ($hadStamp) {
      Copy-Item -LiteralPath $stamp -Destination $stampBackup
    }
    Copy-Item -LiteralPath $Source -Destination $candidate
    if ($hadDestination) {
      try {
        [System.IO.File]::Replace(
          [System.IO.Path]::GetFullPath($candidate),
          [System.IO.Path]::GetFullPath($destination),
          [System.IO.Path]::GetFullPath($backup)
        )
      } catch {
        throw "failed to replace ${destination}: $($_.Exception.Message)"
      }
    } else {
      Move-Item -LiteralPath $candidate -Destination $destination
    }
    $replacementCompleted = $true
    Set-Content -LiteralPath $stamp -NoNewline -Value $version
    if ($target -ne "unknown") {
      Set-Content -LiteralPath $targetFile -NoNewline -Value $target
    }
  } catch {
    $installFailure = $_
    if ($replacementCompleted) {
      try {
        if ($hadDestination) {
          Remove-Item -LiteralPath $destination -Force
          Move-Item -LiteralPath $backup -Destination $destination
        } else {
          Remove-Item -LiteralPath $destination -Force
        }
        if ($hadStamp) {
          Remove-Item -LiteralPath $stamp -Force -ErrorAction SilentlyContinue
          Move-Item -LiteralPath $stampBackup -Destination $stamp
        } else {
          Remove-Item -LiteralPath $stamp -Force -ErrorAction SilentlyContinue
        }
        $replacementCompleted = $false
      } catch {
        $keepRecoveryFiles = $true
        throw (
          "$($installFailure.Exception.Message); rollback also failed: " +
          $_.Exception.Message
        )
      }
    }
    throw $installFailure
  } finally {
    Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    if (-not $keepRecoveryFiles) {
      Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
      Remove-Item -LiteralPath $stampBackup -Force -ErrorAction SilentlyContinue
    }
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
  if ($target -eq "unknown") {
    throw "no plannotator-tui release target for Windows/$architecture"
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
