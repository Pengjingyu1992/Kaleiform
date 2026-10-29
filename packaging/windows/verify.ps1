<# Validate x64 installer/portable packages on a disposable Windows test runner. #>
param(
  [string] $Dist = 'dist\release',
  [string] $Output = 'dist\windows-qa'
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
python (Join-Path $PSScriptRoot 'privacy.py') $zips[0].FullName
if ($LASTEXITCODE -ne 0) { throw 'Windows package privacy checks failed' }

function Assert-Package([string] $Directory) {
  foreach ($file in 'vectorcraft.exe', 'vectorcraft-cli.exe', 'docs\NOTICE', 'docs\UPSTREAM.md',
      'docs\licenses\LICENSE-linesweeper-MIT', 'docs\licenses\LICENSE-wgpu-hal-MIT', 'docs\licenses\LICENSE-lucide.txt') {
    if (-not (Test-Path -LiteralPath (Join-Path $Directory $file))) { throw "Package is missing $file" }
  }
}

function Invoke-Smoke([string] $Directory, [string] $Name) {
  Assert-Package $Directory
  python (Join-Path $Root 'packaging\smoke.py') --bin-dir $Directory --desktop --output (Join-Path $Output $Name)
  if ($LASTEXITCODE -ne 0) { throw "$Name desktop/CLI checks failed" }
}

function Invoke-Msi([string] $Operation, [string] $Log) {
  $msiArguments = @($Operation, "`"$($msis[0].FullName)`"", '/qn', '/norestart', '/l*v', "`"$Log`"")
  $install = Start-Process msiexec.exe -ArgumentList $msiArguments -Wait -PassThru
  if ($install.ExitCode -notin 0, 3010) {
    Get-Content -LiteralPath $Log -Tail 60
    throw "msiexec $Operation failed: $($install.ExitCode)"
  }
}

$directory = Join-Path $env:ProgramFiles 'Kaleiform'
if (Test-Path -LiteralPath (Join-Path $directory 'vectorcraft.exe')) { throw 'Use a disposable runner without an existing Kaleiform installation' }
$installed = $false
try {
  Invoke-Msi '/i' (Join-Path $Output 'install.log')
  $installed = $true
  Invoke-Smoke $directory 'installed'
  foreach ($extension in '.vectorcraft', '.drawcraft', '.vctemplate') {
    $key = Get-Item -LiteralPath "Registry::HKEY_CLASSES_ROOT\$extension"
    if ($key.GetValue('') -ne 'Vectorcraft.Document') { throw "Missing document association: $extension" }
  }
} finally {
  if ($installed) { Invoke-Msi '/x' (Join-Path $Output 'uninstall.log') }
}
if (Test-Path -LiteralPath (Join-Path $directory 'vectorcraft.exe')) { throw 'Uninstall left the app executable behind' }
$portable = Join-Path $Output 'portable'
Expand-Archive -LiteralPath $zips[0].FullName -DestinationPath $portable -Force
$apps = @(Get-ChildItem -LiteralPath $portable -Filter 'vectorcraft.exe' -Recurse)
if ($apps.Count -ne 1) { throw 'Portable archive must contain one desktop executable' }
Invoke-Smoke $apps[0].DirectoryName 'portable-check'
Write-Output 'Windows x64: SHA-256, MSI install/desktop/file association/uninstall and portable desktop checks passed'
