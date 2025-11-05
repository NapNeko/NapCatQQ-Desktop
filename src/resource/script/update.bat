@echo off
setlocal enabledelayedexpansion

rem 更新脚本 — 更健壮的实现
rem 功能：提权（UAC）尝试、检测 runtime\tmp 中的 NapCatQQ-Desktop*.exe、支持已安装可执行的变体、等待并在超时后强制结束、move/copy 回退、最小化启动、新增日志，以及尽量自删除。

rem 日志文件
set "log=%~dp0update.log"

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

rem 新旧程序路径与文件名检测
set "new_app_dir=%~dp0runtime\tmp"
set "new_app_path="
set "new_file_name="

rem 确保临时目录存在
if not exist "%new_app_dir%" (
    mkdir "%new_app_dir%"
    echo [%date% %time%] 创建目录 %new_app_dir% >> "%log%"
)

rem 在临时目录查找新的可执行文件（支持通配符）
for %%F in ("%new_app_dir%\NapCatQQ-Desktop*.exe") do (
    set "new_app_path=%%~fF"
    set "new_file_name=%%~nxF"
    goto :found_new
)
:found_new
if not defined new_app_path (
    echo [%date% %time%] 新程序未找到: %new_app_dir%\NapCatQQ-Desktop*.exe >> "%log%"
    goto :end
)

rem 确定已安装目录下的可执行（优先替换已存在文件名），否则使用新文件名
set "installed_app_path="
set "installed_file_name="
for %%G in ("%~dp0NapCatQQ-Desktop*.exe") do (
    set "installed_app_path=%%~fG"
    set "installed_file_name=%%~nxG"
    goto :found_installed
)
:found_installed
if not defined installed_app_path (
    set "installed_file_name=%new_file_name%"
    set "installed_app_path=%~dp0%new_file_name%"
)

rem 进程名用于检测运行的旧进程
set "proc_name=%installed_file_name%"

rem 等待旧版进程退出（最多等待60秒，可根据需要调整）
set /a max_wait=60
set /a waited=0
:wait_for_exit
tasklist /FI "IMAGENAME eq %proc_name%" /NH | find /I "%proc_name%" >NUL
if "%ERRORLEVEL%"=="0" (
    if %waited% GEQ %max_wait% (
        echo [%date% %time%] 等待超时: %waited% 秒，尝试强制结束 %proc_name% >> "%log%"
        taskkill /IM "%proc_name%" /F >> "%log%" 2>&1
        timeout /T 1 /NOBREAK > NUL
    ) else (
        set /a waited+=1
        timeout /T 1 /NOBREAK > NUL
        goto wait_for_exit
    )
)

rem 尝试移动新文件到应用目录（优先用 move，失败则复制覆盖）
echo [%date% %time%] 尝试移动 %new_app_path% 到 %installed_app_path% >> "%log%"
if exist "%new_app_path%" (
    move /Y "%new_app_path%" "%installed_app_path%" >> "%log%" 2>&1
    if errorlevel 1 (
        echo [%date% %time%] move 失败，尝试复制覆盖 >> "%log%"
        copy /Y "%new_app_path%" "%installed_app_path%" >> "%log%" 2>&1
        if errorlevel 1 (
            echo [%date% %time%] 复制失败，放弃更新 >> "%log%"
            goto :end
        ) else (
            del /Q "%new_app_path%" >> "%log%" 2>&1
        )
    )
) else (
    echo [%date% %time%] 新程序未找到（移动前）: %new_app_path% >> "%log%"
    goto :end
)

rem 启动新版本（最小化）
echo [%date% %time%] 启动新程序: %installed_app_path% >> "%log%"
start "" /MIN "%installed_app_path%" >> "%log%" 2>&1

rem 尝试删除自身：优先直接删除，失败则启动一个最小化的 cmd 来延迟删除
del "%~f0" >> "%log%" 2>&1
if errorlevel 1 (
    echo [%date% %time%] 无法直接删除自身，尝试延迟删除 >> "%log%"
    start "" /MIN cmd /c "ping 127.0.0.1 -n 3 >nul && del \"%~f0\"" >> "%log%" 2>&1
)

:end
echo [%date% %time%] 更新结束 >> "%log%"
endlocal
