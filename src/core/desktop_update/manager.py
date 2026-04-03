# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 更新执行器。"""

import ctypes
import logging
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from src.core.desktop_update.constants import (
    MINIMUM_MSI_SIZE_BYTES,
    MSI_LOG_FILE,
    MSI_UPDATE_SCRIPT_FILENAME,
)
from src.core.installation.install_type import InstallType, detect_install_type, get_update_file_pattern
from src.core.desktop_update.templates import load_msi_update_script
from src.core.platform.app_paths import resolve_app_data_path

logger = logging.getLogger(__name__)


@dataclass(slots=True, frozen=True)
class UpdateLaunchResult:
    """更新启动结果。"""

    pid: int | None = None


class MsiUpdateStrategy:
    """MSI 安装版更新策略。"""

    PID_NONE_MARKER = "0"

    def prepare(self, package_path: Path, base_path: Path) -> Path:
        if not package_path.is_file():
            raise ValueError(f"MSI 文件不存在: {package_path}")

        if package_path.suffix.lower() != ".msi":
            raise ValueError(f"不是有效的 MSI 文件: {package_path}")

        if package_path.stat().st_size < MINIMUM_MSI_SIZE_BYTES:
            raise ValueError(f"MSI 文件太小，可能已损坏: {package_path}")

        package_path = package_path.resolve()
        last_error: OSError | None = None
        for tmp_dir in self._iter_candidate_tmp_dirs(base_path):
            staged_msi_path = tmp_dir / package_path.name
            try:
                tmp_dir.mkdir(parents=True, exist_ok=True)
                if package_path.parent == tmp_dir.resolve():
                    logger.info(f"MSI 文件已位于 staging 目录，无需复制: {package_path}")
                    return package_path

                if staged_msi_path.exists():
                    staged_msi_path.unlink()
                shutil.copy2(package_path, staged_msi_path)
                logger.info(f"MSI 文件已复制到: {staged_msi_path}")
                return staged_msi_path
            except OSError as exc:
                last_error = exc
                logger.warning(f"复制 MSI 文件到 {tmp_dir} 失败，尝试回退目录: {exc}")

        raise ValueError(f"复制 MSI 文件失败: {last_error}") from last_error

    def get_update_script_path(self, staging_path: Path) -> Path:
        return staging_path.parent / MSI_UPDATE_SCRIPT_FILENAME

    def load_update_script(self) -> str:
        """读取 MSI 更新脚本模板。"""

        return load_msi_update_script()

    @staticmethod
    def _build_log_line(message: str) -> str:
        timestamp = datetime.now().strftime("%Y/%m/%d %H:%M:%S")
        return f"[{timestamp}] {message}\n"

    def _write_update_log(self, log_path: Path, message: str, *, overwrite: bool = False) -> None:
        mode = "w" if overwrite else "a"
        try:
            log_path.parent.mkdir(parents=True, exist_ok=True)
            with log_path.open(mode, encoding="utf-8", newline="\r\n") as file:
                file.write(self._build_log_line(message))
        except OSError as exc:
            logger.warning(f"写入 MSI 更新日志失败: {exc}")

    @staticmethod
    def build_msi_launch_command(staging_path: Path, app_root: Path) -> tuple[str, str]:
        system_root = Path(os.environ.get("SystemRoot", r"C:\Windows"))
        msiexec_path = system_root / "System32" / "msiexec.exe"
        arguments = subprocess.list2cmdline(
            [
                "/i",
                str(staging_path),
                "/quiet",
                "/norestart",
                "/l*v",
                str(app_root / "msi_install.log"),
            ]
        )
        return str(msiexec_path), arguments

    @staticmethod
    def _is_running_as_admin() -> bool:
        try:
            return bool(ctypes.windll.shell32.IsUserAnAdmin())
        except (AttributeError, OSError):
            return False

    def _launch_msi_process(self, executable: str, arguments: str, cwd: str) -> UpdateLaunchResult:
        verb = "open" if self._is_running_as_admin() else "runas"
        result = ctypes.windll.shell32.ShellExecuteW(None, verb, executable, arguments, cwd, 1)
        if result <= 32:
            raise OSError(f"ShellExecuteW failed with code {result}")
        return UpdateLaunchResult()

    def execute_update(self, staging_path: Path, app_root: Path, target_pid: int | None = None) -> UpdateLaunchResult | None:
        log_path = staging_path.parent / MSI_LOG_FILE

        if not staging_path.is_file():
            self._write_update_log(log_path, f"MSI 文件不存在，无法启动更新: {staging_path}", overwrite=True)
            logger.error(f"启动 MSI 更新失败: MSI 文件不存在: {staging_path}")
            return None

        executable, arguments = self.build_msi_launch_command(staging_path, app_root)
        self._write_update_log(
            log_path,
            f"准备直接启动 MSI 更新: exe={executable}, msi={staging_path}, target_pid={target_pid or self.PID_NONE_MARKER}",
            overwrite=True,
        )

        try:
            result = self._launch_msi_process(
                executable=executable,
                arguments=arguments,
                cwd=str(staging_path.parent),
            )
            self._write_update_log(log_path, "MSI 进程已成功启动，当前应用将退出。")
            logger.info(f"MSI 更新进程已启动: exe={executable}, msi={staging_path}")
            return result
        except OSError as exc:
            self._write_update_log(log_path, f"启动 MSI 失败: {exc}")
            logger.error(f"启动 MSI 更新失败: {exc}")
            return None

    @staticmethod
    def _iter_candidate_tmp_dirs(base_path: Path) -> tuple[Path, ...]:
        return (
            base_path / "runtime" / "tmp",
            Path(tempfile.gettempdir()) / "NapCatQQ-Desktop" / "update",
        )


class UpdateManager:
    """统一更新管理器。

    应用内 Desktop 更新已统一收敛为 MSI 安装包：
    - MSI 安装版执行原地升级
    - 便携版/未知安装类型通过 MSI 安装包迁移到 MSI 安装版
    """

    def __init__(self, base_path: Path | None = None):
        if base_path is None:
            base_path = _get_default_base_path()

        self.base_path = base_path.resolve()
        self.data_path = resolve_app_data_path()
        self.install_type = detect_install_type(self.base_path)
        self._strategy = MsiUpdateStrategy()

        logger.info(
            "更新管理器初始化: "
            f"current_install_type={self.install_type.value}, update_channel=msi, "
            f"base_path={self.base_path}, data_path={self.data_path}"
        )

    @property
    def install_type(self) -> InstallType:
        return self._install_type

    @install_type.setter
    def install_type(self, value: InstallType) -> None:
        self._install_type = value

    def get_update_filename(self, version: str) -> str:
        return get_update_file_pattern(InstallType.MSI, version)

    def prepare_update(self, package_path: Path) -> Path:
        return self._strategy.prepare(package_path, self.data_path)

    def get_update_script_path(self, staging_path: Path) -> Path | None:
        return self._strategy.get_update_script_path(staging_path)

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> UpdateLaunchResult | None:
        return self._strategy.execute_update(staging_path, self.base_path, target_pid)

    def is_msi_installation(self) -> bool:
        return self._install_type == InstallType.MSI

    def requires_msi_migration(self) -> bool:
        return self._install_type != InstallType.MSI


def _get_default_base_path() -> Path:
    import sys

    if getattr(sys, "frozen", False):
        return Path(sys.executable).parent
    return Path(__file__).parent.parent.parent.parent

