<# Validate x64 packages in disposable Windows runners, in independently reported phases. #>
param(
  [string] $Dist = 'dist\release',
  [string] $Output = 'dist\windows-qa',
  [ValidateSet('All', 'Validate', 'Install', 'InstalledCli', 'InstalledDesktop', 'Uninstall', 'PortableCli', 'PortableDesktop')]
  [string] $Phase = 'All'
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Dist = (Resolve-Path $Dist).Path
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$Output = (Resolve-Path $Output).Path
$msis = @(Get-ChildItem -LiteralPath $Dist -Filter '*-windows-x64.msi')
$zips = @(Get-ChildItem -LiteralPath $Dist -Filter '*-windows-x64-portable.zip')
$manifests = @(Get-ChildItem -LiteralPath $Dist -Filter '*-windows-x64.sha256')
if ($msis.Count -ne 1 -or $zips.Count -ne 1 -or $manifests.Count -ne 1) { throw 'Expected exactly one x64 MSI, portable ZIP and manifest' }
Write-Output '::notice::Checking Windows package SHA-256'
$expected = @($msis[0].Name, $zips[0].Name)
$seen = @()
foreach ($line in Get-Content -LiteralPath $manifests[0].FullName) {
  if ($line -notmatch '^([0-9a-f]{64})  ([^\/:]+)$') { throw 'Invalid Windows checksum manifest' }
  $digest, $name = $Matches[1], $Matches[2]
  if ($name -notin $expected -or $name -in $seen) { throw 'Unexpected or repeated manifest filename' }
  if ((Get-FileHash -LiteralPath (Join-Path $Dist $name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $digest) { throw "Checksum mismatch: $name" }
  $seen += $name
}
if ($seen.Count -ne 2) { throw 'The manifest did not verify both Windows packages' }
$manifestDigest = (Get-FileHash -LiteralPath $manifests[0].FullName -Algorithm SHA256).Hash
$validated = Join-Path $Output 'validated.sha256'
if ($Phase -in 'All', 'Validate') {
  Write-Output '::notice::Scanning Windows portable payload privacy'
  python (Join-Path $PSScriptRoot 'privacy.py') $zips[0].FullName
  if ($LASTEXITCODE -ne 0) { throw 'Windows package privacy checks failed' }
  [IO.File]::WriteAllText($validated, $manifestDigest)
  Write-Output '::notice::Windows checksums and package privacy passed'
} elseif (-not (Test-Path -LiteralPath $validated) -or (Get-Content -LiteralPath $validated -Raw) -ne $manifestDigest) {
  throw 'Run the Validate phase for these exact packages before runtime checks'
}
if ($Phase -eq 'Validate') { return }

function Assert-Package([string] $Directory) {
  foreach ($file in 'vectorcraft.exe', 'vectorcraft-cli.exe', 'docs\NOTICE', 'docs\UPSTREAM.md',
      'docs\licenses\LICENSE-linesweeper-MIT', 'docs\licenses\LICENSE-wgpu-hal-MIT', 'docs\licenses\LICENSE-lucide.txt') {
    if (-not (Test-Path -LiteralPath (Join-Path $Directory $file))) { throw "Package is missing $file" }
  }
}

function Invoke-Smoke([string] $Directory, [string] $Name, [switch] $Desktop) {
  Assert-Package $Directory
  Write-Output "::notice::Starting $Name checks"
  $smokeArguments = @((Join-Path $Root 'packaging\smoke.py'), '--bin-dir', $Directory, '--output', (Join-Path $Output $Name))
  if ($Desktop) { $smokeArguments += @('--desktop', '--extended') }
  python @smokeArguments
  if ($LASTEXITCODE -ne 0) { throw "$Name checks failed" }
  Write-Output "::notice::$Name checks passed"
}

function Invoke-Msi([string] $Operation, [string] $Log) {
  $msiArguments = @($Operation, "`"$($msis[0].FullName)`"", '/qn', '/norestart', '/l*v', "`"$Log`"")
  Write-Output "::notice::Starting msiexec $Operation"
  # Wait for our client, not every descendant process in a PowerShell job.
  $install = Start-Process msiexec.exe -ArgumentList $msiArguments -PassThru
  $null = $install.Handle
  if (-not $install.WaitForExit(180000)) {
    try { $install.Kill() } catch { Write-Warning 'Could not stop the timed-out installer client' }
    Get-Content -LiteralPath $Log -Tail 60 -ErrorAction SilentlyContinue
    throw "msiexec $Operation did not exit within 180 seconds"
  }
  if ($install.ExitCode -notin 0, 3010) {
    Get-Content -LiteralPath $Log -Tail 60
    throw "msiexec $Operation failed: $($install.ExitCode)"
  }
  Write-Output "::notice::msiexec $Operation passed"
}

$directory = Join-Path $env:ProgramFiles 'Kaleiform'
$ownedInstall = Join-Path $Output 'installed.sha256'
function Install-App {
  if (Test-Path -LiteralPath (Join-Path $directory 'vectorcraft.exe')) { throw 'Use a disposable runner without an existing Kaleiform installation' }
  Invoke-Msi '/i' (Join-Path $Output 'install.log')
  [IO.File]::WriteAllText($ownedInstall, $manifestDigest)
  Assert-Package $directory
  foreach ($extension in '.vectorcraft', '.drawcraft', '.vctemplate') {
    $key = Get-Item -LiteralPath "Registry::HKEY_CLASSES_ROOT\$extension"
    if ($key.GetValue('') -ne 'Vectorcraft.Document') { throw "Missing document association: $extension" }
  }
}
function Uninstall-App {
  if (-not (Test-Path -LiteralPath $ownedInstall) -or (Get-Content -LiteralPath $ownedInstall -Raw) -ne $manifestDigest) {
    throw 'Refusing to uninstall an application not installed by this package check'
  }
  Invoke-Msi '/x' (Join-Path $Output 'uninstall.log')
  if (Test-Path -LiteralPath (Join-Path $directory 'vectorcraft.exe')) { throw 'Uninstall left the app executable behind' }
  Remove-Item -LiteralPath $ownedInstall
}
function Open-Portable {
  $portable = Join-Path $Output 'portable'
  Expand-Archive -LiteralPath $zips[0].FullName -DestinationPath $portable -Force
  $apps = @(Get-ChildItem -LiteralPath $portable -Filter 'vectorcraft.exe' -Recurse)
  if ($apps.Count -ne 1) { throw 'Portable archive must contain one desktop executable' }
  return $apps[0].DirectoryName
}

switch ($Phase) {
  'Install' { Install-App; return }
  'InstalledCli' { Invoke-Smoke $directory 'installed-cli'; return }
  'InstalledDesktop' { Invoke-Smoke $directory 'installed' -Desktop; return }
  'Uninstall' { Uninstall-App; return }
  'PortableCli' { Invoke-Smoke (Open-Portable) 'portable-cli'; return }
  'PortableDesktop' { Invoke-Smoke (Open-Portable) 'portable-check' -Desktop; return }
}
try {
  Install-App
  Invoke-Smoke $directory 'installed' -Desktop
} finally {
  if (Test-Path -LiteralPath $ownedInstall) { Uninstall-App }
}
Invoke-Smoke (Open-Portable) 'portable-check' -Desktop
Write-Output 'Windows x64 package, installed desktop/uninstall and portable desktop checks passed'
