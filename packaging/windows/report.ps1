<# Surface errors without exposing matched secrets from the privacy checker. #>
$ErrorActionPreference = 'Stop'
foreach ($log in Get-ChildItem dist/windows-qa/*.log -Recurse -ErrorAction SilentlyContinue) {
  Get-Content $log.FullName -Tail 100 | Where-Object { $_ -match 'error(?:\[[A-Z0-9]+\])?[\s:]|\bFAILED\b|panicked|AssertionError|Exception|RuntimeError|TimeoutExpired|Return value 3' -and $_ -notmatch '^\s*(Compiling|test result: ok\.)|MSI .*Note: 1: (2205|2228)\b|Installation success or error status: 0\.' } | Select-Object -Last 8 | ForEach-Object {
    $message = "$_".Replace('%', '%25').Replace("`r", '%0D').Replace("`n", '%0A')
    Write-Output "::error::$message"
  }
}
