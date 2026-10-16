# CI 计划任务中的普通用户入口，生成独立测试结果供父任务读取。
param(
    [string]$TestExecutable,
    [Parameter(Mandatory)][string]$ExpectedSid,
    [Parameter(Mandatory)][string]$ResultPath,
    [Parameter(Mandatory)][string]$LogPath,
    [switch]$ProbeOnly
)
$ErrorActionPreference = 'Stop'
$result = @{ exitCode = 1; error = $null }
try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($identity.User.Value -ne $ExpectedSid) { throw 'Scheduled task ran as the wrong user' }
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Scheduled task unexpectedly has an administrator token'
    }
    $env:NCD_AGENT_TEST_OWNER_SID = $ExpectedSid
    $env:TEMP = Split-Path -Parent $ResultPath
    $env:TMP = $env:TEMP
    if ($ProbeOnly) {
        $result.exitCode = 0
    } else {
        & $TestExecutable --test-threads=4 2>&1 | Tee-Object -FilePath $LogPath | Out-Null
        $result.exitCode = $LASTEXITCODE
    }
} catch {
    $result.error = $_.Exception.Message
} finally {
    $result | ConvertTo-Json -Compress | Set-Content -LiteralPath "$ResultPath.tmp" -Encoding utf8
    Move-Item -LiteralPath "$ResultPath.tmp" -Destination $ResultPath
}
exit $result.exitCode
