@echo off
setlocal enabledelayedexpansion

rem MSI 安装版更新脚本
rem 运行位置：runtime\tmp\update_msi.bat
rem 功能：等待旧进程退出，以管理员权限运行 MSI 升级安装

for %%I in ("%~dp0") do set "script_dir=%%~fI"
if not defined target_pid set "target_pid=%~1"
if not defined app_root set "app_root=%~2"
if not defined msi_path set "msi_path=%~3"
if not defined log set "log=%~4"

if "%target_pid%"=="0" set "target_pid="
if "%target_pid%"=="__NONE__" set "target_pid="

if not defined app_root for %%I in ("%script_dir%\..\..") do set "app_root=%%~fI"
if not defined log set "log=%script_dir%\msi_update.log"
if not defined msi_path set "msi_path=%app_root%\runtime\tmp\NapCatQQ-Desktop.msi"

rem ---------- 提权检查 ----------
rem 如果没有管理员权限，则尝试以管理员权限重新启动当前脚本（会触发 UAC）
net session >NUL 2>&1
if "%ERRORLEVEL%" NEQ "0" (
    echo [%date% %time%] 当前未以管理员运行，尝试以管理员权限重新启动 >> "%log%"
    powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath \"%~f0\" -ArgumentList '%*' -Verb RunAs"
    if errorlevel 1 (
        echo [%date% %time%] 无法以管理员权限重新启动，放弃更新 >> "%log%"
        goto :end
    )
    rem 管理员实例已发起，退出当前实例
    goto :end
)
rem ---------- 提权检查 结束 ----------

echo [%date% %time%] MSI 更新开始 > "%log%"

rem 检查 MSI 文件是否存在
if not exist "%msi_path%" (
    echo [%date% %time%] MSI 文件未找到: %msi_path% >> "%log%"
    goto :end
)

rem ---------- 等待旧版进程退出 ----------
rem 最多等待 30 秒（脚本启动前主程序应已退出，这里作为保险）
set /a max_wait=30
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
echo [%date% %time%] 旧版进程已退出，等待时间: %waited% 秒 >> "%log%"
rem ---------- 等待结束 ----------

rem ---------- 执行 MSI 升级安装 ----------
echo [%date% %time%] 开始执行 MSI 升级安装 >> "%log%"

rem 使用 msiexec 进行升级安装
rem /i - 安装
rem /quiet - 静默安装
rem /norestart - 不重启
rem /l*v - 详细日志
start /wait msiexec /i "%msi_path%" /quiet /norestart /l*v "%app_root%\msi_install.log" REINSTALL=ALL REINSTALLMODE=vomus

set "msi_rc=%ERRORLEVEL%"
echo [%date% %time%] MSI 安装完成，返回码: %msi_rc% >> "%log%"

rem 检查安装结果
if %msi_rc% EQU 0 (
    echo [%date% %time%] MSI 升级安装成功 >> "%log%"
    rem 删除已使用的 MSI 文件
    del /F /Q "%msi_path%" >> "%log%" 2>&1
    rem 启动新版本（可选，MSI 通常不需要，因为 MajorUpgrade 会处理）
    rem start "" "%app_root%\NapCatQQ-Desktop.exe"
) else if %msi_rc% EQU 3010 (
    echo [%date% %time%] MSI 升级安装成功，但系统要求重启 >> "%log%"
    del /F /Q "%msi_path%" >> "%log%" 2>&1
) else if %msi_rc% EQU 1641 (
    echo [%date% %time%] MSI 已触发重启并继续安装 >> "%log%"
    del /F /Q "%msi_path%" >> "%log%" 2>&1
) else (
    echo [%date% %time%] MSI 升级安装失败，返回码: %msi_rc% >> "%log%"
    rem 保留 MSI 文件以便排查问题
)

:end
echo [%date% %time%] MSI 更新脚本结束 >> "%log%"
endlocal
