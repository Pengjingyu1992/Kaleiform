<# One target-specific Rust configuration shared by release tests and package builds. #>
param([Parameter(Mandatory = $true)] [string] $Target)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$rustFlags = @('-C', 'target-feature=+crt-static', '-C', 'strip=debuginfo', "--remap-path-prefix=$Root=.")
$toolDirectories = @($env:CARGO_HOME, $env:RUSTUP_HOME)
if ($env:USERPROFILE) { $toolDirectories += (Join-Path $env:USERPROFILE '.cargo'), (Join-Path $env:USERPROFILE '.rustup') }
foreach ($toolDir in $toolDirectories) {
  if ($toolDir) { $rustFlags += "--remap-path-prefix=$toolDir=rust-tools" }
}
# JSON arrays are valid TOML arrays and preserve paths containing spaces.
"target.$Target.rustflags=" + (ConvertTo-Json -InputObject $rustFlags -Compress)
