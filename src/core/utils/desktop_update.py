# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 目录版更新辅助逻辑。"""

# 标准库导入
import os
import stat
import shutil
import time
import zipfile
from pathlib import Path


UPDATE_ARCHIVE_NAME = "NapCatQQ-Desktop.zip"
UPDATE_APP_DIR_NAME = "NapCatQQ-Desktop"
UPDATE_EXE_NAME = "NapCatQQ-Desktop.exe"
UPDATE_STAGING_DIR_NAME = "_update_staging"
UPDATE_STAGING_PACKAGE_DIR = "package"
REQUIRED_UPDATE_ENTRIES = (UPDATE_EXE_NAME, "_internal")


def get_update_staging_root(base_path: Path) -> Path:
    """返回目录版更新 staging 根目录。"""

    return base_path / UPDATE_STAGING_DIR_NAME


def get_update_stage_package_root(base_path: Path) -> Path:
    """返回目录版更新解压目录。"""

    return get_update_staging_root(base_path) / UPDATE_STAGING_PACKAGE_DIR


def get_staged_app_dir(base_path: Path) -> Path:
    """返回 staging 中的应用目录。"""

    return get_update_stage_package_root(base_path) / UPDATE_APP_DIR_NAME


def prepare_desktop_update(zip_path: Path, base_path: Path) -> Path:
    """验证并解压目录版更新包。

    Returns:
        Path: staging 中的应用目录。
    """

    if not zip_path.is_file():
        raise ValueError(f"更新包不存在: {zip_path}")

    staging_root = get_update_staging_root(base_path)
    _clear_staging_root(staging_root)

    package_root = get_update_stage_package_root(base_path)
    package_root.mkdir(parents=True, exist_ok=True)

    with zipfile.ZipFile(zip_path, "r") as archive:
        _validate_archive_members(archive)
        bad_member = archive.testzip()
        if bad_member is not None:
            raise ValueError(f"更新包损坏: {bad_member}")
        archive.extractall(package_root)

    app_dir = get_staged_app_dir(base_path)
    if not app_dir.is_dir():
        raise ValueError(f"更新包结构不正确，缺少目录: {UPDATE_APP_DIR_NAME}")

    missing_entries = [entry for entry in REQUIRED_UPDATE_ENTRIES if not (app_dir / entry).exists()]
    if missing_entries:
        missing_text = ", ".join(f"{UPDATE_APP_DIR_NAME}/{entry}" for entry in missing_entries)
        raise ValueError(f"更新包结构不正确，缺少 {missing_text}")

    return app_dir


def _clear_staging_root(staging_root: Path) -> None:
    """删除旧的 staging 目录，兼容 Windows 只读文件和短暂占用。"""

    if not staging_root.exists():
        return

    last_error: OSError | None = None
    for attempt in range(3):
        try:
            shutil.rmtree(staging_root, onerror=_handle_remove_readonly)
            return
        except OSError as exc:
            last_error = exc
            time.sleep(0.2 * (attempt + 1))

    raise ValueError(f"无法清理旧更新目录: {staging_root}: {last_error}")


def _validate_archive_members(archive: zipfile.ZipFile) -> None:
    """阻止绝对路径和目录穿越条目。"""

    for member in archive.infolist():
        member_path = Path(member.filename)
        if member_path.is_absolute():
            raise ValueError(f"更新包包含非法绝对路径: {member.filename}")
        if ".." in member_path.parts:
            raise ValueError(f"更新包包含非法相对路径: {member.filename}")


def _handle_remove_readonly(func, path: str, exc_info) -> None:
    """清除只读属性后重试删除。"""

    exc = exc_info[1]
    if not isinstance(exc, PermissionError):
        raise exc

    os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
    func(path)
