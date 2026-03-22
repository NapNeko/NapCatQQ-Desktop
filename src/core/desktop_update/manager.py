# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 更新执行器。"""

import logging
import os
import shutil
import stat
import subprocess
import time
import zipfile
from abc import ABC, abstractmethod
from pathlib import Path

from src.core.desktop_update.constants import (
    MINIMUM_MSI_SIZE_BYTES,
    MSI_UPDATE_FILENAME,
    MSI_UPDATE_SCRIPT_FILENAME,
    REQUIRED_UPDATE_ENTRIES,
    UPDATE_APP_DIR_NAME,
    UPDATE_STAGING_DIR_NAME,
    UPDATE_STAGING_PACKAGE_DIR,
)
from src.core.desktop_update.scripts import inject_target_pid
from src.core.installation.install_type import InstallType, detect_install_type, get_update_file_pattern
from src.core.desktop_update.templates import load_msi_update_script

logger = logging.getLogger(__name__)


class UpdateStrategy(ABC):
    """更新策略抽象基类。"""

    @abstractmethod
    def prepare(self, package_path: Path, base_path: Path) -> Path:
        """准备更新包，返回可用于执行更新的路径。"""

    @abstractmethod
    def get_update_script_path(self, staging_path: Path) -> Path | None:
        """获取更新脚本路径。"""

    @abstractmethod
    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        """执行更新。"""


class PortableUpdateStrategy(UpdateStrategy):
    """便携版（ZIP）更新策略。"""

    def prepare(self, package_path: Path, base_path: Path) -> Path:
        if not package_path.is_file():
            raise ValueError(f"更新包不存在: {package_path}")

        staging_root = get_update_staging_root(base_path)
        _clear_staging_root(staging_root)

        package_root = get_update_stage_package_root(base_path)
        package_root.mkdir(parents=True, exist_ok=True)

        with zipfile.ZipFile(package_path, "r") as archive:
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

    def get_update_script_path(self, staging_path: Path) -> Path:
        return get_update_staging_root(staging_path.parent.parent) / "update.bat"

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        script_path = self.get_update_script_path(staging_path)
        if not script_path.exists():
            logger.error(f"更新脚本不存在: {script_path}")
            return None

        cmd = [str(script_path)]
        if target_pid:
            cmd.append(str(target_pid))

        try:
            return subprocess.Popen(
                cmd,
                shell=False,
                cwd=str(script_path.parent),
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_CONSOLE,
            )
        except OSError as exc:
            logger.error(f"启动更新脚本失败: {exc}")
            return None


class MsiUpdateStrategy(UpdateStrategy):
    """MSI 安装版更新策略。"""

    def prepare(self, package_path: Path, base_path: Path) -> Path:
        if not package_path.is_file():
            raise ValueError(f"MSI 文件不存在: {package_path}")

        if package_path.suffix.lower() != ".msi":
            raise ValueError(f"不是有效的 MSI 文件: {package_path}")

        if package_path.stat().st_size < MINIMUM_MSI_SIZE_BYTES:
            raise ValueError(f"MSI 文件太小，可能已损坏: {package_path}")

        tmp_dir = base_path / "runtime" / "tmp"
        tmp_dir.mkdir(parents=True, exist_ok=True)
        standard_msi_path = tmp_dir / MSI_UPDATE_FILENAME

        try:
            shutil.copy2(package_path, standard_msi_path)
            logger.info(f"MSI 文件已复制到: {standard_msi_path}")
        except OSError as exc:
            raise ValueError(f"复制 MSI 文件失败: {exc}") from exc

        return standard_msi_path

    def get_update_script_path(self, staging_path: Path) -> Path:
        return staging_path.parent / MSI_UPDATE_SCRIPT_FILENAME

    def load_update_script(self) -> str:
        """读取 MSI 更新脚本模板。"""

        return load_msi_update_script()

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        script_path = self.get_update_script_path(staging_path)

        try:
            script_content = self.load_update_script()
            if target_pid:
                script_content = inject_target_pid(script_content, target_pid)
            script_path.write_text(script_content, encoding="utf-8")
            logger.info(f"MSI 更新脚本已生成: {script_path}")
        except OSError as exc:
            logger.error(f"写入 MSI 更新脚本失败: {exc}")
            return None

        try:
            return subprocess.Popen(
                [str(script_path)],
                shell=False,
                cwd=str(script_path.parent),
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_CONSOLE,
            )
        except OSError as exc:
            logger.error(f"启动 MSI 更新脚本失败: {exc}")
            return None


class UpdateManager:
    """统一更新管理器。"""

    def __init__(self, base_path: Path | None = None):
        if base_path is None:
            base_path = _get_default_base_path()

        self.base_path = base_path.resolve()
        self.install_type = detect_install_type(self.base_path)
        self._strategy = self._create_strategy()

        logger.info(f"更新管理器初始化: type={self.install_type.value}, path={self.base_path}")

    def _create_strategy(self) -> UpdateStrategy:
        if self.install_type == InstallType.MSI:
            return MsiUpdateStrategy()
        return PortableUpdateStrategy()

    @property
    def install_type(self) -> InstallType:
        return self._install_type

    @install_type.setter
    def install_type(self, value: InstallType) -> None:
        self._install_type = value

    def get_update_filename(self, version: str) -> str:
        return get_update_file_pattern(self._install_type, version)

    def prepare_update(self, package_path: Path) -> Path:
        return self._strategy.prepare(package_path, self.base_path)

    def get_update_script_path(self, staging_path: Path) -> Path | None:
        return self._strategy.get_update_script_path(staging_path)

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        return self._strategy.execute_update(staging_path, target_pid)

    def is_msi_installation(self) -> bool:
        return self._install_type == InstallType.MSI

    def is_portable_installation(self) -> bool:
        return self._install_type == InstallType.PORTABLE


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
    """验证并解压目录版更新包。"""

    return PortableUpdateStrategy().prepare(zip_path, base_path)


def _get_default_base_path() -> Path:
    import sys

    if getattr(sys, "frozen", False):
        return Path(sys.executable).parent
    return Path(__file__).parent.parent.parent.parent


def _clear_staging_root(staging_root: Path) -> None:
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
    for member in archive.infolist():
        member_path = Path(member.filename)
        if member_path.is_absolute():
            raise ValueError(f"更新包包含非法绝对路径: {member.filename}")
        if ".." in member_path.parts:
            raise ValueError(f"更新包包含非法相对路径: {member.filename}")


def _handle_remove_readonly(func, path: str, exc_info) -> None:
    exc = exc_info[1]
    if not isinstance(exc, PermissionError):
        raise exc

    os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
    func(path)

