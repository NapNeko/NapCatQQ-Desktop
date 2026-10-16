# GitHub 托管 Windows runner 上，用临时普通账户执行真实 Agent 回归。
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'This fixture is only for disposable GitHub Windows runners'
}

function Build-AgentTests {
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
    return $artifacts[0].executable
}

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
$fixtureStarted = Get-Date
function Show-FixtureDiagnostics {
    Get-Service -Name Schedule | Format-List Name, Status, StartType | Out-Host
    Get-WinEvent -FilterHashtable @{
        LogName = 'Microsoft-Windows-TaskScheduler/Operational'
        StartTime = $fixtureStarted
    } -MaxEvents 100 -ErrorAction SilentlyContinue |
        Where-Object { $_.Message -like "*$taskName*" } |
        Format-List TimeCreated, Id, Message | Out-Host
}
function Register-FixtureTask([switch]$ProbeOnly) {
    $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden ' +
        "-File `"$wrapper`" -TestExecutable `"$testExecutable`" -ExpectedSid $($account.SID.Value) " +
        "-ResultPath `"$resultPath`" -LogPath `"$logPath`""
    if ($ProbeOnly) { $arguments += ' -ProbeOnly' }
    $action = New-ScheduledTaskAction -Execute (Join-Path $PSHOME 'pwsh.exe') -Argument $arguments -WorkingDirectory $sandbox
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 3) `
        -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable
    $settings.AllowDemandStart = $true
    $settings.Enabled = $true
    $settings.RunOnlyIfIdle = $false
    $settings.RunOnlyIfNetworkAvailable = $false
    $login = "$env:COMPUTERNAME\$username"
    $principal = New-ScheduledTaskPrincipal -UserId $login -LogonType Password -RunLevel Limited
    $trigger = New-ScheduledTaskTrigger -Once -At ((Get-Date).AddSeconds(5))
    $definition = New-ScheduledTask -Action $action -Settings $settings -Principal $principal -Trigger $trigger
    Register-ScheduledTask -TaskName $taskName -InputObject $definition -User $login -Password $password -Force | Out-Null
    $script:taskRegistered = $true
    $registered = Get-ScheduledTask -TaskName $taskName
    if ($registered.Principal.LogonType -ne 'Password' -or $registered.Principal.RunLevel -ne 'Limited') {
        throw 'Unexpected scheduled-task logon configuration'
    }
}

function Invoke-FixtureTask {
    if (Test-Path -LiteralPath $resultPath) { Remove-Item -LiteralPath $resultPath }
    $scheduler = New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    $registeredTask = $scheduler.GetFolder('\').GetTask($taskName)
    try {
        # TASK_RUN_IGNORE_CONSTRAINTS does not change the registered user or token.
        $instance = $registeredTask.RunEx($null, 2, 0, $null)
        if ($null -eq $instance) { throw 'Task Scheduler returned no running instance' }
        Write-Host "Scheduled instance=$($instance.InstanceGuid), state=$($instance.State)"
    } catch {
        Show-FixtureDiagnostics
        throw
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while (-not (Test-Path -LiteralPath $resultPath)) {
        if ([DateTime]::UtcNow -ge $deadline) {
            $info = Get-ScheduledTaskInfo -TaskName $taskName
            $task = Get-ScheduledTask -TaskName $taskName
            Write-Host (Export-ScheduledTask -TaskName $taskName)
            Show-FixtureDiagnostics
            throw "Standard-account task timed out; state=$($task.State), result=$($info.LastTaskResult), lastRun=$($info.LastRunTime)"
        }
        Start-Sleep -Milliseconds 500
    }
    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($result.exitCode -ne 0 -or $result.error) {
        if (Test-Path -LiteralPath $logPath) { Write-Host (Get-Content -LiteralPath $logPath -Raw) }
        throw "Standard-account Agent tests failed: exit=$($result.exitCode), error=$($result.error)"
    }
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    $script:taskRegistered = $false
}

try {
    wevtutil sl Microsoft-Windows-TaskScheduler/Operational /e:true
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enable isolated runner task diagnostics' }
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
    $wrapper = Join-Path $sandbox 'run.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-agent-user.ps1') -Destination $wrapper
    $resultPath = Join-Path $sandbox 'result.json'
    $logPath = Join-Path $sandbox 'tests.log'
    Register-FixtureTask -ProbeOnly
    Invoke-FixtureTask
    Write-Host 'Standard account SID and non-elevated logon verified before compiling'
    $sourceExecutable = Build-AgentTests
    Copy-Item -LiteralPath $sourceExecutable -Destination $testExecutable
    Register-FixtureTask
    Invoke-FixtureTask
    if (Test-Path -LiteralPath $logPath) { Get-Content -LiteralPath $logPath }
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
