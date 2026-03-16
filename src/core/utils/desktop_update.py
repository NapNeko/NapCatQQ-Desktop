# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 目录版更新辅助逻辑。"""

# 标准库导入
import shutil
import zipfile
from pathlib import Path


UPDATE_ARCHIVE_NAME = "NapCatQQ-Desktop.zip"
UPDATE_APP_DIR_NAME = "NapCatQQ-Desktop"
UPDATE_EXE_NAME = "NapCatQQ-Desktop.exe"
UPDATE_STAGING_DIR_NAME = "_update_staging"
UPDATE_STAGING_PACKAGE_DIR = "package"


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
    if staging_root.exists():
        shutil.rmtree(staging_root)

    package_root = get_update_stage_package_root(base_path)
    package_root.mkdir(parents=True, exist_ok=True)

    with zipfile.ZipFile(zip_path, "r") as archive:
        _validate_archive_members(archive)
        bad_member = archive.testzip()
        if bad_member is not None:
            raise ValueError(f"更新包损坏: {bad_member}")
        archive.extractall(package_root)

    app_dir = get_staged_app_dir(base_path)
    exe_path = app_dir / UPDATE_EXE_NAME
    if not app_dir.is_dir() or not exe_path.is_file():
        raise ValueError(f"更新包结构不正确，缺少 {UPDATE_APP_DIR_NAME}/{UPDATE_EXE_NAME}")

    return app_dir


def _validate_archive_members(archive: zipfile.ZipFile) -> None:
    """阻止绝对路径和目录穿越条目。"""

    for member in archive.infolist():
        member_path = Path(member.filename)
        if member_path.is_absolute():
            raise ValueError(f"更新包包含非法绝对路径: {member.filename}")
        if ".." in member_path.parts:
            raise ValueError(f"更新包包含非法相对路径: {member.filename}")
