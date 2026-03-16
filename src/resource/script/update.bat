@echo off
setlocal enabledelayedexpansion

rem 目录版更新脚本
rem 运行位置：runtime\tmp\update.bat
rem 功能：等待旧进程退出，将 staging 中的目录版整包镜像到应用目录，保留 runtime/log 数据，再重启新版本。

for %%I in ("%~dp0") do set "script_dir=%%~fI"
for %%I in ("%script_dir%\..\..") do set "app_root=%%~fI"
set "log=%app_root%\update.log"
set "staged_app_dir=%app_root%\_update_staging\package\NapCatQQ-Desktop"
set "staged_exe=%staged_app_dir%\NapCatQQ-Desktop.exe"
set "installed_exe=%app_root%\NapCatQQ-Desktop.exe"

rem ---------- 提权检查 ----------
rem 如果没有管理员权限，则尝试以管理员权限重新启动当前脚本（会触发 UAC）
net session >NUL 2>&1
if "%ERRORLEVEL%" NEQ "0" (
    echo [%date% %time%] 当前未以管理员运行，尝试以管理员权限重新启动 >> "%log%"
    powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -ArgumentList '%*' -Verb RunAs"
    if errorlevel 1 (
        echo [%date% %time%] 无法以管理员权限重新启动，放弃更新 >> "%log%"
        goto :end
    )
    rem 管理员实例已发起，退出当前实例
    goto :end
)
rem ---------- 提权检查 结束 ----------

echo [%date% %time%] 更新开始 > "%log%"

if not exist "%staged_exe%" (
    echo [%date% %time%] 新目录版程序未找到: %staged_exe% >> "%log%"
    goto :end
)

rem 等待旧版进程退出（最多等待60秒，可根据需要调整）
set /a max_wait=60
set /a waited=0
:wait_for_exit
if defined target_pid (
    tasklist /FI "PID eq %target_pid%" /NH | find /I "%target_pid%" >NUL
) else (
    tasklist /FI "IMAGENAME eq NapCatQQ-Desktop.exe" /NH | find /I "NapCatQQ-Desktop.exe" >NUL
)
if "%ERRORLEVEL%"=="0" (
    if %waited% GEQ %max_wait% (
        if defined target_pid (
            echo [%date% %time%] 等待 PID %target_pid% 退出超时: %waited% 秒，尝试强制结束 >> "%log%"
            taskkill /PID "%target_pid%" /F >> "%log%" 2>&1
        ) else (
            echo [%date% %time%] 等待 NapCatQQ-Desktop.exe 退出超时: %waited% 秒，尝试强制结束 >> "%log%"
            taskkill /IM "NapCatQQ-Desktop.exe" /F >> "%log%" 2>&1
        )
        timeout /T 1 /NOBREAK > NUL
    ) else (
        set /a waited+=1
        timeout /T 1 /NOBREAK > NUL
        goto wait_for_exit
    )
)

rem 使用 robocopy 镜像目录版程序，保留 runtime 和 log 数据目录
echo [%date% %time%] 开始镜像目录版更新包到 %app_root% >> "%log%"
robocopy "%staged_app_dir%" "%app_root%" /MIR /R:3 /W:1 /NFL /NDL /NP /XD "%app_root%\runtime" "%app_root%\log" "%app_root%\_update_staging" >> "%log%" 2>&1
set "copy_rc=%ERRORLEVEL%"
if %copy_rc% GEQ 8 (
    echo [%date% %time%] robocopy 失败，错误码: %copy_rc% >> "%log%"
    goto :end
)

if not exist "%installed_exe%" (
    echo [%date% %time%] 更新后未找到主程序: %installed_exe% >> "%log%"
    goto :end
)

del /Q "%app_root%\runtime\tmp\NapCatQQ-Desktop.zip" >> "%log%" 2>&1
rmdir /S /Q "%app_root%\_update_staging" >> "%log%" 2>&1

rem 启动新版本
echo [%date% %time%] 启动新程序: %installed_exe% >> "%log%"
start "" /MIN "%installed_exe%" >> "%log%" 2>&1

rem 尝试删除自身：优先直接删除，失败则启动一个最小化的 cmd 来延迟删除
del "%~f0" >> "%log%" 2>&1
if errorlevel 1 (
    echo [%date% %time%] 无法直接删除自身，尝试延迟删除 >> "%log%"
    start "" /MIN cmd /c "ping 127.0.0.1 -n 3 >nul && del \"%~f0\"" >> "%log%" 2>&1
)

:end
echo [%date% %time%] 更新结束 >> "%log%"
endlocal
