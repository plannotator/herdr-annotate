$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$version = (Get-Content "herdr-annotate.version" -Raw).Trim()
if (-not $version) { throw "herdr-annotate.version is empty" }
New-Item -ItemType Directory -Force "bin" | Out-Null
$destination = Join-Path "bin" "herdr-annotate.exe"
$stamp = Join-Path "bin" "herdr-annotate.version"
$installed = if (Test-Path $stamp) { (Get-Content $stamp -Raw).Trim() } else { "" }

if ((Test-Path $destination) -and $installed -eq $version -and -not $env:HERDR_ANNOTATE_BIN) {
  Write-Output "herdr-annotate $version already installed"
  exit 0
}

if ($env:HERDR_ANNOTATE_BIN) {
  if (-not (Test-Path $env:HERDR_ANNOTATE_BIN -PathType Leaf)) {
    throw "HERDR_ANNOTATE_BIN is not a file: $env:HERDR_ANNOTATE_BIN"
  }
  Copy-Item -Force $env:HERDR_ANNOTATE_BIN "$destination.tmp"
  Move-Item -Force "$destination.tmp" $destination
  Set-Content -NoNewline $stamp $version
  Write-Output "installed herdr-annotate from $env:HERDR_ANNOTATE_BIN (local build, stamped $version)"
  exit 0
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$target = switch ($architecture) {
  "X64" { "x86_64-pc-windows-msvc" }
  "Arm64" { "aarch64-pc-windows-msvc" }
  default { throw "no native Herdr Annotate Lite build for Windows/$architecture" }
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
  if ($actual -ne $expected) { throw "sha256 mismatch for $asset: expected $expected, got $actual" }
  Copy-Item -Force (Join-Path $temporary $asset) "$destination.tmp"
  Move-Item -Force "$destination.tmp" $destination
  Set-Content -NoNewline $stamp $version
  Write-Output "installed herdr-annotate $version ($target)"
}
finally {
  Remove-Item -Recurse -Force $temporary -ErrorAction SilentlyContinue
}
