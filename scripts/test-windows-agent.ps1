# GitHub 托管 Windows runner 上，用临时普通账户执行真实 Agent 回归。
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'This fixture is only for disposable GitHub Windows runners'
}

$messages = @(cargo test --workspace --exclude ncd-tauri --no-run --message-format=json)
if ($LASTEXITCODE -ne 0) {
    $messages | Write-Output
    throw 'Workspace test compilation failed'
}
$artifacts = @($messages | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
    $_.reason -eq 'compiler-artifact' -and $_.profile.test -and
    $_.target.name -eq 'ncd-agent' -and $_.executable
})
if ($artifacts.Count -ne 1) { throw 'Expected exactly one Agent test executable' }

$nonce = [Guid]::NewGuid().ToString('N')
$username = "NcdTest$($nonce.Substring(0, 8))"
$taskName = "NcdAgentCI-$nonce"
$publicRoot = [IO.Path]::GetFullPath($env:PUBLIC)
$sandbox = [IO.Path]::GetFullPath((Join-Path $publicRoot $taskName))
if ([IO.Path]::GetDirectoryName($sandbox) -ne $publicRoot.TrimEnd('\')) {
    throw 'Invalid isolated test directory'
}
$password = "Ncd!$([Guid]::NewGuid().ToString('N'))Aa1"
Write-Output "::add-mask::$password"
$securePassword = ConvertTo-SecureString $password -AsPlainText -Force
$account = $null
$taskRegistered = $false
$sandboxCreated = $false
try {
    $account = New-LocalUser -Name $username -Password $securePassword -AccountNeverExpires -PasswordNeverExpires
    $users = Get-LocalGroup -SID 'S-1-5-32-545'
    Add-LocalGroupMember -Group $users -Member $account
    New-Item -ItemType Directory -Path $sandbox | Out-Null
    $sandboxCreated = $true
    $acl = Get-Acl -LiteralPath $sandbox
    $rule = [Security.AccessControl.FileSystemAccessRule]::new(
        $account.SID, 'Modify', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
    )
    $acl.AddAccessRule($rule)
    Set-Acl -LiteralPath $sandbox -AclObject $acl
    $testExecutable = Join-Path $sandbox 'ncd-agent-tests.exe'
    Copy-Item -LiteralPath $artifacts[0].executable -Destination $testExecutable
    $wrapper = Join-Path $sandbox 'run.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-agent-user.ps1') -Destination $wrapper
    $resultPath = Join-Path $sandbox 'result.json'
    $logPath = Join-Path $sandbox 'tests.log'
    $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden ' +
        "-File `"$wrapper`" -TestExecutable `"$testExecutable`" -ExpectedSid $($account.SID.Value) " +
        "-ResultPath `"$resultPath`" -LogPath `"$logPath`""
    $action = New-ScheduledTaskAction -Execute (Join-Path $PSHOME 'pwsh.exe') -Argument $arguments -WorkingDirectory $sandbox
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 3)
    Register-ScheduledTask -TaskName $taskName -Action $action -Settings $settings `
        -User "$env:COMPUTERNAME\$username" -Password $password -RunLevel Limited | Out-Null
    $taskRegistered = $true
    Start-ScheduledTask -TaskName $taskName
    $deadline = [DateTime]::UtcNow.AddSeconds(180)
    while (-not (Test-Path -LiteralPath $resultPath)) {
        if ([DateTime]::UtcNow -ge $deadline) {
            $info = Get-ScheduledTaskInfo -TaskName $taskName
            throw "Standard-account Agent tests timed out; task result=$($info.LastTaskResult)"
        }
        Start-Sleep -Milliseconds 500
    }
    if (Test-Path -LiteralPath $logPath) { Get-Content -LiteralPath $logPath }
    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($result.exitCode -ne 0 -or $result.error) {
        throw "Standard-account Agent tests failed: exit=$($result.exitCode), error=$($result.error)"
    }
    $output = Get-Content -LiteralPath $logPath -Raw
    if ($output -notmatch 'test result: ok\. [1-9][0-9]* passed; 0 failed; 0 ignored;') {
        throw 'Agent test executable did not complete its tests'
    }
} finally {
    if ($taskRegistered) {
        Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    if ($account) { Remove-LocalUser -SID $account.SID }
    $securePassword.Dispose()
    if ($sandboxCreated -and (Test-Path -LiteralPath $sandbox)) {
        $resolved = [IO.Path]::GetFullPath($sandbox)
        $directory = Get-Item -LiteralPath $resolved
        if ([IO.Path]::GetDirectoryName($resolved) -ne $publicRoot.TrimEnd('\') -or
            $directory.Name -ne $taskName -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Refusing to clean an unexpected test directory'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
