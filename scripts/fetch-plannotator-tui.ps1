# -DestinationDirectory stages the binary and its stamp somewhere other than the repository's
# own bin/, which is how the windows-full variant keeps its copy beside its manifest. Omitted,
# the destination is unchanged, so every existing caller behaves exactly as before. The pin is
# read from the repository root either way: there is one release pin, not one per variant.
param([string]$DestinationDirectory)

$ErrorActionPreference = "Stop"
# Anchored on the script's own location rather than the current directory, and every path
# below is absolute and literal. A checkout path may legitimately contain square brackets --
# the Windows acceptance path matrix requires it -- and PowerShell reads those as wildcards in
# any -Path parameter. Worse, once the current directory contains them, it is stored escaped,
# so even -LiteralPath with a relative path resolves to a name with backticks in it and is not
# found. Not depending on the current directory at all is what makes that whole class go away.
# .NET rather than Split-Path: an extended-length \\?\ root is a path matrix row, and
# Split-Path cannot parse one -- it reports a null drive and returns nothing.
$root = [System.IO.Path]::GetDirectoryName($PSScriptRoot)

$versionContents = Get-Content -LiteralPath ([System.IO.Path]::Combine($root, "plannotator-tui.version")) -Raw
$version = if ($null -eq $versionContents) { "" } else { [string]$versionContents }
$version = $version.Trim()
if (-not $version) { throw "plannotator-tui.version is empty" }

$destinationDirectory = if ([string]::IsNullOrWhiteSpace($DestinationDirectory)) {
  [System.IO.Path]::Combine($root, "bin")
} else {
  $DestinationDirectory
}
$destination = [System.IO.Path]::Combine($destinationDirectory, "plannotator-tui.exe")
$stamp = [System.IO.Path]::Combine($destinationDirectory, "plannotator-tui.version")
# .NET rather than New-Item for the same reason: the destination is an absolute path that may
# contain brackets, and New-Item -Path would treat them as a wildcard.
[System.IO.Directory]::CreateDirectory($destinationDirectory) | Out-Null

$localOverride = [Environment]::GetEnvironmentVariable("PLANNOTATOR_TUI_BIN", "Process")
# Empty counts as absent, as it does for PLANNOTATOR_TUI_RELEASE_BASE below and in the
# Unix fetcher: a caller clearing the name through an API that binds $null as
# "" leaves it defined, and an empty override would otherwise reach the "is not a file"
# throw, which sits outside the warn-and-exit-zero path and fails the build outright.
$hasLocalOverride = -not [string]::IsNullOrWhiteSpace($localOverride)
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

  $candidate = [System.IO.Path]::Combine($destinationDirectory, ("plannotator-tui-" + [guid]::NewGuid() + ".tmp"))
  $backup = [System.IO.Path]::Combine($destinationDirectory, ("plannotator-tui-" + [guid]::NewGuid() + ".bak"))
  $stampBackup = [System.IO.Path]::Combine($destinationDirectory, ("plannotator-tui-version-" + [guid]::NewGuid() + ".bak"))
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
  # PLANNOTATOR_TUI_RELEASE_BASE is a test-only seam for a loopback fixture server. An empty
  # value counts as absent: a caller clearing it through an API that binds $null as "" would
  # otherwise leave the name defined, and an empty base builds a URL with no host at all --
  # which surfaces as "invalid URI" long after the mistake, through the warn-and-exit-zero
  # contract that hides it.
  $base = if (-not [string]::IsNullOrWhiteSpace($releaseBaseOverride)) {
    $releaseBaseOverride.TrimEnd([char]"/")
  } else {
    "https://github.com/plannotator/plannotator-tui/releases/download/v$version"
  }

  $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("plannotator-tui-" + [guid]::NewGuid())
  try {
    [System.IO.Directory]::CreateDirectory($temporary) | Out-Null
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

# Explicit rather than falling off the end: the windows-full wrapper propagates
# $LASTEXITCODE, which a script ending without an exit leaves undefined or at whatever
# the last native command set.
exit 0
