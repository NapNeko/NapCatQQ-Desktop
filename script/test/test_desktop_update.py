# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.desktop_update.manager as desktop_update_manager
from src.core.desktop_update import MINIMUM_MSI_SIZE_BYTES, MSI_UPDATE_FILENAME, MsiUpdateStrategy, UpdateManager
from src.core.desktop_update.scripts import inject_target_pid
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

    assert "Start-Process -FilePath '%~f0' -ArgumentList '%*' -Verb RunAs" in script_content
    assert 'set "msi_path=%app_root%\\runtime\\tmp\\NapCatQQ-Desktop.msi"' in script_content
    assert 'msiexec /i "%msi_path%" /quiet /norestart' in script_content


def test_inject_target_pid_inserts_after_setlocal() -> None:
    """target_pid 应注入到 setlocal 后，避免污染脚本头部。"""

    content = "@echo off\nsetlocal enabledelayedexpansion\necho update\n"

    result = inject_target_pid(content, 114514)

    assert 'set "target_pid=114514"' in result
    assert result.index('set "target_pid=114514"') > result.index("setlocal enabledelayedexpansion")
