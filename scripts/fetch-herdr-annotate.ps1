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

$versionContents = Get-Content -LiteralPath ([System.IO.Path]::Combine($root, "herdr-annotate.version")) -Raw
$version = if ($null -eq $versionContents) { "" } else { [string]$versionContents }
$version = $version.Trim()
if (-not $version) { throw "herdr-annotate.version is empty" }
$binDirectory = [System.IO.Path]::Combine($root, "bin")
[System.IO.Directory]::CreateDirectory($binDirectory) | Out-Null
$destination = [System.IO.Path]::Combine($binDirectory, "herdr-annotate.exe")
$stamp = [System.IO.Path]::Combine($binDirectory, "herdr-annotate.version")
$installed = if (Test-Path -LiteralPath $stamp -PathType Leaf) { ([string](Get-Content -LiteralPath $stamp -Raw)).Trim() } else { "" }

if ((Test-Path -LiteralPath $destination) -and $installed -eq $version -and -not $env:HERDR_ANNOTATE_BIN) {
  Write-Output "herdr-annotate $version already installed"
  exit 0
}

if ($env:HERDR_ANNOTATE_BIN) {
  if (-not (Test-Path -LiteralPath $env:HERDR_ANNOTATE_BIN -PathType Leaf)) {
    throw "HERDR_ANNOTATE_BIN is not a file: $env:HERDR_ANNOTATE_BIN"
  }
  Copy-Item -Force -LiteralPath $env:HERDR_ANNOTATE_BIN -Destination "$destination.tmp"
  Move-Item -Force -LiteralPath "$destination.tmp" -Destination $destination
  Set-Content -LiteralPath $stamp -NoNewline -Value $version
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
[System.IO.Directory]::CreateDirectory($temporary) | Out-Null
try {
  Invoke-WebRequest -UseBasicParsing "$base/$asset" -OutFile (Join-Path $temporary $asset)
  Invoke-WebRequest -UseBasicParsing "$base/SHA256SUMS" -OutFile (Join-Path $temporary "SHA256SUMS")
  $line = Get-Content -LiteralPath (Join-Path $temporary "SHA256SUMS") | Where-Object { $_ -match "\s$([regex]::Escape($asset))$" } | Select-Object -First 1
  if (-not $line) { throw "$asset is not listed in $base/SHA256SUMS" }
  $expected = ($line -split "\s+")[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $temporary $asset)).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "sha256 mismatch for ${asset}: expected $expected, got $actual" }
  Copy-Item -Force -LiteralPath (Join-Path $temporary $asset) -Destination "$destination.tmp"
  Move-Item -Force -LiteralPath "$destination.tmp" -Destination $destination
  Set-Content -LiteralPath $stamp -NoNewline -Value $version
  Write-Output "installed herdr-annotate $version ($target)"
}
finally {
  Remove-Item -Recurse -Force -LiteralPath $temporary -ErrorAction SilentlyContinue
}
