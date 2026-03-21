@echo off
setlocal enabledelayedexpansion

rem 这是远端迁移脚本入口模板。
rem 注意：客户端当前只会下载并执行这一个 bat 文件。
rem 所以生产脚本必须自包含，不要依赖仓库里的相邻文件一定存在。

if not defined target_pid (
    echo [ERROR] missing target_pid
    exit /b 1
)

for %%I in ("%~dp0") do set "script_dir=%%~fI"

rem 如果脚本由客户端下载到临时目录，不能直接依赖 %~dp0 推导安装目录。
rem 生产脚本应改成你真实的 app_root 定位方式。
set "app_root=C:\Path\To\NapCatQQ-Desktop"

set "log=%TEMP%\napcat_desktop_migration.log"
set "backup_root=%app_root%\_update_staging\migration_backup"
set "backup_app_dir=%backup_root%\NapCatQQ-Desktop"

echo [INFO] migration template started > "%log%"
echo [INFO] target_pid=%target_pid% >> "%log%"
echo [INFO] app_root=%app_root% >> "%log%"

rem 1. 等待主程序退出
tasklist /FI "PID eq %target_pid%" | findstr /I "%target_pid%" >nul
if %errorlevel%==0 (
    timeout /t 2 /nobreak >nul
)

rem 2. 预检
if not exist "%app_root%" (
    echo [ERROR] app_root not found: %app_root% >> "%log%"
    exit /b 1
)

rem 3. 备份
if exist "%backup_root%" rd /s /q "%backup_root%"
mkdir "%backup_app_dir%" >> "%log%" 2>&1
robocopy "%app_root%" "%backup_app_dir%" /MIR >> "%log%" 2>&1
if errorlevel 8 goto rollback

rem 4. 在这里插入你的迁移逻辑
rem 示例：
rem - move "%app_root%\old-config" "%app_root%\config"
rem - del /q "%app_root%\obsolete.json"
rem - powershell -ExecutionPolicy Bypass -File "inline or generated helper"

echo [INFO] template migration finished >> "%log%"
exit /b 0

:rollback
echo [WARN] migration failed, rolling back >> "%log%"
if exist "%backup_app_dir%" (
    robocopy "%backup_app_dir%" "%app_root%" /MIR >> "%log%" 2>&1
)
exit /b 1
