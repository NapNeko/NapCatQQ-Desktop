# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 更新执行器。"""

import logging
import shutil
import subprocess
import tempfile
from pathlib import Path

from src.core.desktop_update.constants import (
    MINIMUM_MSI_SIZE_BYTES,
    MSI_LOG_FILE,
    MSI_UPDATE_FILENAME,
    MSI_UPDATE_SCRIPT_FILENAME,
)
from src.core.desktop_update.scripts import inject_script_variables
from src.core.installation.install_type import InstallType, detect_install_type, get_update_file_pattern
from src.core.desktop_update.templates import load_msi_update_script
from src.core.platform.app_paths import resolve_app_data_path

logger = logging.getLogger(__name__)


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

        last_error: OSError | None = None
        for tmp_dir in self._iter_candidate_tmp_dirs(base_path):
            standard_msi_path = tmp_dir / MSI_UPDATE_FILENAME
            try:
                tmp_dir.mkdir(parents=True, exist_ok=True)
                if standard_msi_path.exists():
                    standard_msi_path.unlink()
                shutil.copy2(package_path, standard_msi_path)
                logger.info(f"MSI 文件已复制到: {standard_msi_path}")
                return standard_msi_path
            except OSError as exc:
                last_error = exc
                logger.warning(f"复制 MSI 文件到 {tmp_dir} 失败，尝试回退目录: {exc}")

        raise ValueError(f"复制 MSI 文件失败: {last_error}") from last_error

    def get_update_script_path(self, staging_path: Path) -> Path:
        return staging_path.parent / MSI_UPDATE_SCRIPT_FILENAME

    def load_update_script(self) -> str:
        """读取 MSI 更新脚本模板。"""

        return load_msi_update_script()

    def execute_update(
        self, staging_path: Path, app_root: Path, target_pid: int | None = None
    ) -> subprocess.Popen | None:
        script_path = self.get_update_script_path(staging_path)

        try:
            script_content = self.load_update_script()
            script_variables: dict[str, str | int] = {
                "app_root": str(app_root),
                "msi_path": str(staging_path),
                "log": str(staging_path.parent / MSI_LOG_FILE),
            }
            if target_pid is not None:
                script_variables["target_pid"] = target_pid

            script_content = inject_script_variables(script_content, script_variables)
            script_path.write_text(script_content, encoding="utf-8", newline="\r\n")
            logger.info(f"MSI 更新脚本已生成: {script_path}")
        except OSError as exc:
            logger.error(f"写入 MSI 更新脚本失败: {exc}")
            return None

        command = [str(script_path)]
        if target_pid is not None:
            command.append(str(target_pid))
        else:
            command.append(self.PID_NONE_MARKER)
        command.append(str(app_root))
        command.append(str(staging_path))
        command.append(str(staging_path.parent / MSI_LOG_FILE))

        try:
            process = subprocess.Popen(
                command,
                shell=False,
                cwd=str(script_path.parent),
                creationflags=subprocess.CREATE_NEW_CONSOLE,
            )
            return process
        except OSError as exc:
            logger.error(f"启动 MSI 更新脚本失败: {exc}")
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

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
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

