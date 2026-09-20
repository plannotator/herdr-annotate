$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$versionContents = Get-Content -LiteralPath "herdr-annotate.version" -Raw
$version = if ($null -eq $versionContents) { "" } else { [string]$versionContents }
$version = $version.Trim()
if (-not $version) { throw "herdr-annotate.version is empty" }
New-Item -ItemType Directory -Force "bin" | Out-Null
$destination = Join-Path "bin" "herdr-annotate.exe"
$stamp = Join-Path "bin" "herdr-annotate.version"
$targetFile = Join-Path "bin" "herdr-annotate.target"
$installed = if (Test-Path -LiteralPath $stamp -PathType Leaf) { ([string](Get-Content -LiteralPath $stamp -Raw)).Trim() } else { "" }
$installedTarget = if (Test-Path -LiteralPath $targetFile -PathType Leaf) { ([string](Get-Content -LiteralPath $targetFile -Raw)).Trim() } else { "" }

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

if ((Test-Path -LiteralPath $destination -PathType Leaf) -and $installed -eq $version -and (Test-RunnableAndMatches) -and -not $env:HERDR_ANNOTATE_BIN) {
  if (-not (Test-Path -LiteralPath $targetFile -PathType Leaf) -and $target -ne "unknown") {
    Set-Content -LiteralPath $targetFile -NoNewline -Value $target
  }
  Write-Output "herdr-annotate $version already installed"
  exit 0
}

function Invoke-FallbackBuild {
  $cargoToml = Join-Path "rust" "Cargo.toml"
  $cargo = Get-Command "cargo" -ErrorAction SilentlyContinue
  if ((Test-Path -LiteralPath $cargoToml -PathType Leaf) -and $null -ne $cargo) {
    Write-Warning "download failed or no prebuilt available; falling back to cargo build"
    & cargo build --manifest-path $cargoToml --release
    if ($LASTEXITCODE -eq 0) {
      $built = Join-Path "rust" "target\release\herdr-annotate.exe"
      if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
        $built = Join-Path "rust" "target\release\herdr-annotate"
      }
      if (Test-Path -LiteralPath $built -PathType Leaf) {
        Copy-Item -Force -LiteralPath $built -Destination "$destination.tmp"
        Move-Item -Force -LiteralPath "$destination.tmp" -Destination $destination
        Set-Content -LiteralPath $stamp -NoNewline -Value $version
        if ($target -ne "unknown") {
          Set-Content -LiteralPath $targetFile -NoNewline -Value $target
        }
        Write-Output "installed herdr-annotate from fallback build (stamped $version ($target))"
        return $true
      }
    }
  }
  return $false
}

if ($env:HERDR_ANNOTATE_BIN) {
  if (-not (Test-Path $env:HERDR_ANNOTATE_BIN -PathType Leaf)) {
    throw "HERDR_ANNOTATE_BIN is not a file: $env:HERDR_ANNOTATE_BIN"
  }
  Copy-Item -Force -LiteralPath $env:HERDR_ANNOTATE_BIN -Destination "$destination.tmp"
  Move-Item -Force -LiteralPath "$destination.tmp" -Destination $destination
  Set-Content -LiteralPath $stamp -NoNewline -Value $version
  if ($target -ne "unknown") {
    Set-Content -LiteralPath $targetFile -NoNewline -Value $target
  }
  Write-Output "installed herdr-annotate from $env:HERDR_ANNOTATE_BIN (local build, stamped $version)"
  exit 0
}

if ($target -eq "unknown") {
  if (Invoke-FallbackBuild) { exit 0 }
  throw "no native Herdr Annotate Lite build for Windows/$architecture"
}

$asset = "herdr-annotate-$target.exe"
$base = "https://github.com/plannotator/herdr-annotate/releases/download/rust-lite-v$version"
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("herdr-annotate-" + [guid]::NewGuid())
New-Item -ItemType Directory $temporary | Out-Null
try {
  Invoke-WebRequest -UseBasicParsing "$base/$asset" -OutFile (Join-Path $temporary $asset)
  Invoke-WebRequest -UseBasicParsing "$base/SHA256SUMS" -OutFile (Join-Path $temporary "SHA256SUMS")
  $line = Get-Content (Join-Path $temporary "SHA256SUMS") | Where-Object { $_ -match "\s$([regex]::Escape($asset))$" } | Select-Object -First 1
  if (-not $line) { throw "$asset is not listed in $base/SHA256SUMS" }
  $expected = ($line -split "\s+")[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $temporary $asset)).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "sha256 mismatch for ${asset}: expected $expected, got $actual" }
  Copy-Item -Force (Join-Path $temporary $asset) "$destination.tmp"
  Move-Item -Force -LiteralPath "$destination.tmp" -Destination $destination
  Set-Content -LiteralPath $stamp -NoNewline -Value $version
  Set-Content -LiteralPath $targetFile -NoNewline -Value $target
  Write-Output "installed herdr-annotate $version ($target)"
}
catch {
  if (Invoke-FallbackBuild) { exit 0 }
  throw $_
}
finally {
  Remove-Item -Recurse -Force $temporary -ErrorAction SilentlyContinue
}
