# §3's path matrix: the same review-pane lifecycle driven from awkward plugin roots and
# review folders. The lifecycle itself is not restated here -- each case shells out to
# test-herdr-windows-full-pane.ps1, so what is proved per case is exactly what is proved in
# the default case: real fetch, render of a per-run marker, `q`, clean teardown.
#
# Local-drive cases are required: a failure fails the run rather than downgrading to a note.
# UNC is attempted against a loopback share where one is reachable and reported UNVERIFIED
# with its limitation otherwise, since a loopback share is not a remote file server.
#
# Measured lengths and the machine's long-path setting are printed, because every result here
# depends on them and the acceptance record asks for the actual numbers.
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
  $PSNativeCommandUseErrorActionPreference = $false
}

$paneScript = Join-Path $PSScriptRoot "test-herdr-windows-full-pane.ps1"
$temporaryBase = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { $env:TEMP } else { $env:RUNNER_TEMP }
$suiteRoot = Join-Path $temporaryBase ("herdr full paths " + [guid]::NewGuid())

$herdrVersion = "0.9.0"
$herdrSha256 = "b4508c445de1c1a68c760a01735da2aba2fa214b2aafd4b07f732e49b2a64b11"

$longPaths = (Get-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
    -Name LongPathsEnabled -ErrorAction SilentlyContinue).LongPathsEnabled
Write-Output "windows $([Environment]::OSVersion.Version) $env:PROCESSOR_ARCHITECTURE, LongPathsEnabled=$longPaths"

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

$results = [ordered]@{}

try {
  New-Item -ItemType Directory -Force $suiteRoot | Out-Null

  $archive = Join-Path $suiteRoot "herdr.zip"
  Invoke-WebRequest -UseBasicParsing `
    "https://github.com/herdrdev/herdr/releases/download/v$herdrVersion/herdr-windows-x86_64.zip" `
    -OutFile $archive
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
  Assert-True ($hash -ceq $herdrSha256) "pinned Herdr $herdrVersion checksum differs: $hash"
  Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $suiteRoot "herdr")
  $herdr = Get-ChildItem -LiteralPath (Join-Path $suiteRoot "herdr") -Filter "herdr.exe" -File -Recurse |
    Select-Object -First 1
  Assert-True ($null -ne $herdr) "pinned Herdr archive contains no herdr.exe"

  # Every character class §3 names, in one segment: spaces, Unicode, an apostrophe, an
  # ampersand, parentheses, a dollar, a percent and brackets.
  $awkward = "pä th 'x' & (y) `$z %w [v]"

  # Long enough that the plugin root plus \bin\plannotator-tui.exe passes the legacy limit.
  $filler = ("deep-" + ("n" * 40))
  $longRoot = Join-Path $suiteRoot (($filler, $filler, $filler) -join "\")

  $cases = @(
    @{ Label = "special-characters"; Checkout = (Join-Path $suiteRoot $awkward); Review = (Join-Path $suiteRoot "$awkward review"); Required = $true }
    # Not required, and deliberately so. The manifest's build command passes a relative
    # -File argument, and Herdr resolves only the program against the plugin root, not the
    # arguments. Windows PowerShell then resolves that argument against the pane cwd under
    # MAX_PATH, so a long plugin root cannot find a script that plainly exists. The checklist
    # forbids fixing this by changing the root manifest, so it is measured and reported.
    @{ Label = "long-path"; Checkout = $longRoot; Review = (Join-Path $suiteRoot "long review"); Required = $false }
  )

  # An extended-length spelling of an ordinary drive path. Staging still goes through the
  # ordinary path, because that is what Herdr's build step does -- Herdr normalises the root
  # it stores. Only the spelling handed to `plugin link` is extended, and Herdr must resolve
  # it to the same directory and run the same lifecycle from it.
  $extendedCheckout = Join-Path $suiteRoot "extended checkout"
  $cases += @{
    Label    = "extended-length-root"
    Checkout = $extendedCheckout
    Review   = (Join-Path $suiteRoot "extended review")
    LinkPath = ("\\?\" + (Join-Path $extendedCheckout "windows-full"))
    Required = $true
  }

  $uncBase = "\\localhost\Users\" + $env:USERNAME
  $uncUsable = $false
  try {
    $uncProbe = Join-Path $uncBase ("herdr-unc-probe-" + [guid]::NewGuid().ToString("N") + ".tmp")
    Set-Content -LiteralPath $uncProbe -Value "probe" -ErrorAction Stop
    Remove-Item -LiteralPath $uncProbe -Force -ErrorAction SilentlyContinue
    $uncUsable = $true
  } catch {
    $uncUsable = $false
  }
  if ($uncUsable) {
    $uncRoot = Join-Path $uncBase ("herdr-full-unc-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    $cases += @{ Label = "unc-loopback"; Checkout = (Join-Path $uncRoot "checkout"); Review = (Join-Path $uncRoot "review"); Required = $false }
  } else {
    $results["unc-loopback"] = "UNVERIFIED: no writable loopback share on this host"
  }

  foreach ($case in $cases) {
    $variantRoot = Join-Path $case.Checkout "windows-full"
    $binary = Join-Path $variantRoot "bin\plannotator-tui.exe"
    Write-Output ""
    Write-Output "--- $($case.Label): plugin root $($variantRoot.Length) chars, binary $($binary.Length) chars"
    try {
      $linkPath = if ($case.Contains("LinkPath")) { [string]$case.LinkPath } else { "" }
      & $paneScript -Checkout $case.Checkout -Review $case.Review `
        -HerdrExecutable $herdr.FullName -LinkPath $linkPath -Label $case.Label
      if ($LASTEXITCODE -ne 0) { throw "pane lifecycle exited $LASTEXITCODE" }
      $results[$case.Label] = "PASS (root $($variantRoot.Length), binary $($binary.Length))"
    } catch {
      if ($case.Required) { throw "$($case.Label) failed: $($_.Exception.Message)" }
      $results[$case.Label] = "LIMITATION: $($_.Exception.Message)"
    }
  }

  Write-Output ""
  Write-Output "path matrix:"
  foreach ($key in $results.Keys) { Write-Output ("  {0,-20} {1}" -f $key, $results[$key]) }
  $unmet = @($results.Values | Where-Object { $_ -notmatch "^PASS" })
  if ($unmet.Count -gt 0) {
    Write-Output ""
    Write-Output "NOT CLAIMED AS PASSING, reported for review rather than skipped:"
    foreach ($key in $results.Keys) {
      if ($results[$key] -notmatch "^PASS") { Write-Output ("  {0}: {1}" -f $key, $results[$key]) }
    }
  }
  Assert-True (@($results.Values | Where-Object { $_ -notmatch "^(PASS|UNVERIFIED|LIMITATION)" }).Count -eq 0) `
    "a path case reported no classification"
} finally {
  Remove-Item -LiteralPath $suiteRoot -Recurse -Force -ErrorAction SilentlyContinue
  if ($uncUsable -and $null -ne $uncRoot) {
    Remove-Item -LiteralPath $uncRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
