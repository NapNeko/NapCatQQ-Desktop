param(
    [Parameter(Mandatory = $true)]
    [string]$SourceRoot,

    [Parameter(Mandatory = $true)]
    [string]$BackupRoot
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $SourceRoot)) {
    throw "SourceRoot not found: $SourceRoot"
}

if (Test-Path -LiteralPath $BackupRoot) {
    Remove-Item -LiteralPath $BackupRoot -Recurse -Force
}

New-Item -ItemType Directory -Path $BackupRoot -Force | Out-Null
Copy-Item -LiteralPath $SourceRoot -Destination $BackupRoot -Recurse -Force

Write-Output "backup_data ok: $BackupRoot"
