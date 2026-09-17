# Stages plannotator-tui into windows-full/bin/ by handing the shared fetcher an explicit
# destination. Both paths are derived from this script's own location, so the build does not
# depend on the caller's working directory, on HERDR_PLUGIN_ROOT being exported, or on the
# temporary checkout path surviving the install. The shared fetcher keeps the single release
# pin at the repository root and its own default destination for every other caller.
$ErrorActionPreference = "Stop"

$variantRoot = Split-Path -Parent $PSScriptRoot
$repositoryRoot = Split-Path -Parent $variantRoot
$shared = Join-Path $repositoryRoot "scripts\fetch-plannotator-tui.ps1"
if (-not (Test-Path -LiteralPath $shared -PathType Leaf)) {
  throw "shared fetcher not found at $shared"
}

& $shared -DestinationDirectory (Join-Path $variantRoot "bin")

# The shared fetcher warns and exits zero when a download, checksum, architecture or
# replacement step fails, so that Lite stays available. Propagating its status keeps that
# contract rather than inventing one here.
exit $LASTEXITCODE
