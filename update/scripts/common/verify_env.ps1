param(
    [Parameter(Mandatory = $true)]
    [string]$AppRoot
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $AppRoot)) {
    throw "AppRoot not found: $AppRoot"
}

$exePath = Join-Path $AppRoot "NapCatQQ-Desktop.exe"
if (-not (Test-Path -LiteralPath $exePath)) {
    throw "Desktop executable not found: $exePath"
}

Write-Output "verify_env ok: $AppRoot"
