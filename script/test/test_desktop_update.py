# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path
import subprocess

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.desktop_update.manager as desktop_update_manager
from src.core.desktop_update import MINIMUM_MSI_SIZE_BYTES, MSI_UPDATE_FILENAME, MsiUpdateStrategy, UpdateManager
from src.core.desktop_update.constants import MSI_LOG_FILE, MSI_UPDATE_SCRIPT_FILENAME
from src.core.desktop_update.scripts import inject_script_variables, inject_target_pid
from src.core.desktop_update.templates import load_msi_update_script
from src.core.installation.install_type import InstallType


def test_msi_update_strategy_copies_package_to_runtime_tmp(tmp_path: Path) -> None:
    """MSI 更新应将安装包复制到统一的 runtime/tmp 目录。"""

    package_path = tmp_path / "release.msi"
    package_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    staging_path = MsiUpdateStrategy().prepare(package_path, tmp_path)

    assert staging_path == tmp_path / "runtime" / "tmp" / MSI_UPDATE_FILENAME
    assert staging_path.read_bytes() == package_path.read_bytes()


def test_msi_update_strategy_rejects_non_msi_file(tmp_path: Path) -> None:
    """非 MSI 文件不应进入统一更新主线。"""

    package_path = tmp_path / "release.zip"
    package_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    with pytest.raises(ValueError, match="不是有效的 MSI 文件"):
        MsiUpdateStrategy().prepare(package_path, tmp_path)


def test_msi_update_strategy_falls_back_to_system_temp_when_app_dir_unwritable(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """安装目录不可写时应回退到系统临时目录。"""

    package_path = tmp_path / "release.msi"
    package_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)
    fallback_root = tmp_path / "system-temp"
    original_copy2 = desktop_update_manager.shutil.copy2

    def fake_copy2(src, dst, *args, **kwargs):
        if "runtime" in str(dst):
            raise PermissionError("denied")
        return original_copy2(src, dst, *args, **kwargs)

    monkeypatch.setattr(desktop_update_manager.tempfile, "gettempdir", lambda: str(fallback_root))
    monkeypatch.setattr(desktop_update_manager.shutil, "copy2", fake_copy2)

    staging_path = MsiUpdateStrategy().prepare(package_path, tmp_path)

    assert staging_path == fallback_root / "NapCatQQ-Desktop" / "update" / MSI_UPDATE_FILENAME
    assert staging_path.read_bytes() == package_path.read_bytes()


def test_update_manager_always_uses_msi_strategy_for_portable_install(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """即使当前是便携版，应用内更新也必须统一切到 MSI。"""

    monkeypatch.setattr(desktop_update_manager, "detect_install_type", lambda _path: InstallType.PORTABLE)

    manager = UpdateManager(tmp_path)

    assert manager.install_type == InstallType.PORTABLE
    assert manager.requires_msi_migration() is True
    assert manager.get_update_filename("2.0.0") == "NapCatQQ-Desktop-2.0.0-x64.msi"
    assert isinstance(manager._strategy, MsiUpdateStrategy)


def test_load_msi_update_script_contains_msiexec_flow() -> None:
    """MSI 模板必须包含提权与 msiexec 升级命令。"""

    script_content = load_msi_update_script()
    normalized = script_content.replace("\r\n", "\n")

    assert "UAC.ShellExecute" in script_content
    assert "runas" in script_content
    assert 'set "msi_path=%app_root%\\runtime\\tmp\\NapCatQQ-Desktop.msi"' in script_content
    assert 'set "msi_path=%app_root%\\runtime\\tmp\\NapCatQQ-Desktop.msi"' in script_content
    assert '"%SystemRoot%\\System32\\msiexec.exe" /i "%msi_path%" /quiet /norestart' in script_content
    assert 'echo [%date% %time%] MSI 升级安装成功 >> "%log%"\n    rem 删除已使用的 MSI 文件\n    del /F /Q "%msi_path%" >> "%log%" 2>&1\n    rem 启动新版本（可选，MSI 通常不需要，因为 MajorUpgrade 会处理）\n    rem start "" "%app_root%\\NapCatQQ-Desktop.exe"\n    goto :end' in normalized


def test_inject_script_variables_inserts_multiple_lines_after_setlocal() -> None:
    content = "@echo off\nsetlocal enabledelayedexpansion\necho update\n"

    result = inject_script_variables(content, {"target_pid": 114514, "app_root": r"C:\Program Files\NapCat"})

    assert result.index('set "target_pid=114514"') > result.index("setlocal enabledelayedexpansion")
    assert 'set "app_root=C:\\Program Files\\NapCat"' in result


def test_inject_target_pid_inserts_after_setlocal() -> None:
    """target_pid 应注入到 setlocal 后，避免污染脚本头部。"""

    content = "@echo off\nsetlocal enabledelayedexpansion\necho update\n"

    result = inject_target_pid(content, 114514)

    assert 'set "target_pid=114514"' in result
    assert result.index('set "target_pid=114514"') > result.index("setlocal enabledelayedexpansion")


def test_msi_update_strategy_execute_update_passes_pid_args_and_uses_new_console(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """启动脚本时应传递 PID/路径参数，并仅使用新控制台标志。"""

    strategy = MsiUpdateStrategy()
    staging_path = tmp_path / MSI_UPDATE_FILENAME
    staging_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)
    captured: dict[str, object] = {}

    class FakeProcess:
        pid = 9527
        returncode = None

        @staticmethod
        def poll():
            return None

    monkeypatch.setattr(strategy, "load_update_script", lambda: "@echo off\nsetlocal enabledelayedexpansion\necho hi\n")
    def fake_popen(command, shell, cwd, creationflags):
        captured["command"] = command
        captured["shell"] = shell
        captured["cwd"] = cwd
        captured["creationflags"] = creationflags
        return FakeProcess()

    monkeypatch.setattr(desktop_update_manager.subprocess, "Popen", fake_popen)

    process = strategy.execute_update(staging_path, tmp_path, target_pid=114514)

    assert process is not None
    assert captured["shell"] is False
    assert captured["cwd"] == str(tmp_path)
    assert captured["creationflags"] == subprocess.CREATE_NEW_CONSOLE
    assert captured["command"] == [
        str(tmp_path / MSI_UPDATE_SCRIPT_FILENAME),
        "114514",
        str(tmp_path),
        str(staging_path),
        str(tmp_path / MSI_LOG_FILE),
    ]

    script_content = (tmp_path / MSI_UPDATE_SCRIPT_FILENAME).read_text(encoding="utf-8")
    assert 'set "target_pid=114514"' in script_content
    assert f'set "msi_path={staging_path}"' in script_content
    assert f'set "log={tmp_path / MSI_LOG_FILE}"' in script_content


def test_msi_update_strategy_execute_update_accepts_quick_exit_after_launch(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """脚本进程即使很快退出，也不应被误判为启动失败。"""

    strategy = MsiUpdateStrategy()
    staging_path = tmp_path / MSI_UPDATE_FILENAME
    staging_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    class FakeProcess:
        pid = 9529
        returncode = 0

        @staticmethod
        def poll():
            return 0

    monkeypatch.setattr(strategy, "load_update_script", lambda: "@echo off\nsetlocal enabledelayedexpansion\necho hi\n")
    monkeypatch.setattr(desktop_update_manager.subprocess, "Popen", lambda *args, **kwargs: FakeProcess())

    process = strategy.execute_update(staging_path, tmp_path, target_pid=114514)

    assert process is not None
    assert process.returncode == 0


def test_msi_update_strategy_execute_update_uses_pid_none_marker_when_target_pid_missing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """未提供 PID 时应传递显式占位符，而不是空字符串。"""

    strategy = MsiUpdateStrategy()
    staging_path = tmp_path / MSI_UPDATE_FILENAME
    staging_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)
    captured: dict[str, object] = {}

    class FakeProcess:
        pid = 9528
        returncode = None

        @staticmethod
        def poll():
            return None

    monkeypatch.setattr(strategy, "load_update_script", lambda: "@echo off\nsetlocal enabledelayedexpansion\necho hi\n")
    def fake_popen(command, shell, cwd, creationflags):
        captured["command"] = command
        return FakeProcess()

    monkeypatch.setattr(desktop_update_manager.subprocess, "Popen", fake_popen)

    process = strategy.execute_update(staging_path, tmp_path, target_pid=None)

    assert process is not None
    assert captured["command"][1] == MsiUpdateStrategy.PID_NONE_MARKER
