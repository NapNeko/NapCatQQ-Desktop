# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.desktop_update.manager as desktop_update_manager
from src.core.desktop_update import MINIMUM_MSI_SIZE_BYTES, MSI_UPDATE_FILENAME, MsiUpdateStrategy, UpdateLaunchResult, UpdateManager
from src.core.desktop_update.constants import MSI_LOG_FILE
from src.core.desktop_update.scripts import inject_script_variables, inject_target_pid
from src.core.desktop_update.templates import load_msi_update_script
from src.core.installation.install_type import InstallType


def test_msi_update_strategy_preserves_package_filename_when_copying_to_runtime_tmp(tmp_path: Path) -> None:
    """MSI 更新应保留原始文件名，不再强制改成固定名。"""

    package_path = tmp_path / "downloads" / "NapCatQQ-Desktop-2.0.13-x64.msi"
    package_path.parent.mkdir(parents=True)
    package_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    staging_path = MsiUpdateStrategy().prepare(package_path, tmp_path)

    assert staging_path == tmp_path / "runtime" / "tmp" / package_path.name
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

    package_path = tmp_path / "downloads" / "NapCatQQ-Desktop-2.0.13-x64.msi"
    package_path.parent.mkdir(parents=True)
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

    assert staging_path == fallback_root / "NapCatQQ-Desktop" / "update" / package_path.name
    assert staging_path.read_bytes() == package_path.read_bytes()


def test_msi_update_strategy_reuses_package_already_in_runtime_tmp(tmp_path: Path) -> None:
    """下载文件已在 runtime/tmp 时应直接复用该版本号包。"""

    package_path = tmp_path / "runtime" / "tmp" / "NapCatQQ-Desktop-2.0.13-x64.msi"
    package_path.parent.mkdir(parents=True)
    package_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    staging_path = MsiUpdateStrategy().prepare(package_path, tmp_path)

    assert staging_path == package_path


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
    assert "REINSTALL=ALL" not in script_content
    assert "REINSTALLMODE=vomus" not in script_content
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


def test_msi_update_strategy_builds_msiexec_launch_command(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """应直接构造 msiexec 命令，而不是依赖 bat 中转。"""

    monkeypatch.setenv("SystemRoot", r"C:\Windows")
    staging_path = tmp_path / "NapCatQQ-Desktop-2.0.13-x64.msi"
    expected_arguments = desktop_update_manager.subprocess.list2cmdline(
        [
            "/i",
            str(staging_path),
            "/quiet",
            "/norestart",
            "/l*v",
            str(tmp_path / "msi_install.log"),
        ]
    )

    executable, arguments = MsiUpdateStrategy.build_msi_launch_command(
        staging_path,
        tmp_path,
    )

    assert executable == r"C:\Windows\System32\msiexec.exe"
    assert arguments == expected_arguments


def test_msi_update_strategy_execute_update_launches_msiexec_directly(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """更新应直接拉起 msiexec，并记录诊断日志。"""

    strategy = MsiUpdateStrategy()
    staging_path = tmp_path / "NapCatQQ-Desktop-2.0.13-x64.msi"
    staging_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)
    captured: dict[str, object] = {}
    expected_arguments = desktop_update_manager.subprocess.list2cmdline(
        [
            "/i",
            str(staging_path),
            "/quiet",
            "/norestart",
            "/l*v",
            str(tmp_path / "msi_install.log"),
        ]
    )

    monkeypatch.setenv("SystemRoot", r"C:\Windows")

    def fake_launch(executable: str, arguments: str, cwd: str) -> UpdateLaunchResult:
        captured["executable"] = executable
        captured["arguments"] = arguments
        captured["cwd"] = cwd
        return UpdateLaunchResult(pid=9527)

    monkeypatch.setattr(strategy, "_launch_msi_process", fake_launch)

    result = strategy.execute_update(staging_path, tmp_path, target_pid=114514)

    assert result == UpdateLaunchResult(pid=9527)
    assert captured["executable"] == r"C:\Windows\System32\msiexec.exe"
    assert captured["arguments"] == expected_arguments
    assert captured["cwd"] == str(tmp_path)

    update_log = (tmp_path / MSI_LOG_FILE).read_text(encoding="utf-8")
    assert "准备直接启动 MSI 更新" in update_log
    assert "target_pid=114514" in update_log
    assert "MSI 进程已成功启动" in update_log


def test_msi_update_strategy_execute_update_logs_failure_when_launch_fails(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """直接启动 msiexec 失败时应返回 None 并记录原因。"""

    strategy = MsiUpdateStrategy()
    staging_path = tmp_path / "NapCatQQ-Desktop-2.0.13-x64.msi"
    staging_path.write_bytes(b"0" * MINIMUM_MSI_SIZE_BYTES)

    monkeypatch.setenv("SystemRoot", r"C:\Windows")
    monkeypatch.setattr(strategy, "_launch_msi_process", lambda *_args, **_kwargs: (_ for _ in ()).throw(OSError("denied")))

    result = strategy.execute_update(staging_path, tmp_path, target_pid=None)

    assert result is None
    update_log = (tmp_path / MSI_LOG_FILE).read_text(encoding="utf-8")
    assert "target_pid=0" in update_log
    assert "启动 MSI 失败: denied" in update_log
