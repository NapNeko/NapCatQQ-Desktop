# -*- coding: utf-8 -*-
"""NapCatQQ Desktop 更新管理模块。

支持两种更新方式：
1. 便携版（ZIP）：解压替换，使用 update.bat 脚本
2. MSI 安装版：调用 msiexec 进行升级安装
"""

# 标准库导入
import enum
import logging
import os
import shutil
import stat
import subprocess
import time
import zipfile
from abc import ABC, abstractmethod
from pathlib import Path

# 第三方库导入
import httpx

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.install_type import InstallType, detect_install_type, get_update_file_pattern

logger = logging.getLogger(__name__)

# 便携版更新常量（旧常量保留向后兼容）
UPDATE_ARCHIVE_NAME = "NapCatQQ-Desktop.zip"  # 旧常量，新代码使用 UpdateManager
UPDATE_APP_DIR_NAME = "NapCatQQ-Desktop"
UPDATE_EXE_NAME = "NapCatQQ-Desktop.exe"
UPDATE_STAGING_DIR_NAME = "_update_staging"
UPDATE_STAGING_PACKAGE_DIR = "package"
REQUIRED_UPDATE_ENTRIES = (UPDATE_EXE_NAME, "_internal")

# MSI 更新常量
MSI_LOG_FILE = "msi_update.log"


class UpdateStrategy(ABC):
    """更新策略抽象基类。"""

    @abstractmethod
    def prepare(self, package_path: Path, base_path: Path) -> Path:
        """准备更新包，返回可用于执行更新的路径。

        Args:
            package_path: 下载的更新包路径
            base_path: 应用根目录

        Returns:
            Path: 准备好的更新入口路径
        """
        pass

    @abstractmethod
    def get_update_script_path(self, staging_path: Path) -> Path | None:
        """获取更新脚本路径（如果有）。

        Args:
            staging_path: prepare 返回的路径

        Returns:
            Path | None: 脚本路径，无需脚本时返回 None
        """
        pass

    @abstractmethod
    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        """执行更新。

        Args:
            staging_path: prepare 返回的路径
            target_pid: 要等待退出的进程 ID

        Returns:
            subprocess.Popen | None: 更新进程，失败返回 None
        """
        pass


class PortableUpdateStrategy(UpdateStrategy):
    """便携版（ZIP）更新策略。"""

    def prepare(self, package_path: Path, base_path: Path) -> Path:
        """解压 ZIP 包到 staging 目录。"""
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
        """返回 update.bat 脚本路径。"""
        return get_update_staging_root(staging_path.parent.parent) / "update.bat"

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        """执行便携版更新（通过 update.bat）。"""
        script_path = self.get_update_script_path(staging_path)

        if not script_path.exists():
            logger.error(f"更新脚本不存在: {script_path}")
            return None

        # 构建命令行
        cmd = [str(script_path)]
        if target_pid:
            cmd.append(str(target_pid))

        # 以管理员权限启动（update.bat 内部也会检查，但这里显式请求更可靠）
        try:
            return subprocess.Popen(
                cmd,
                shell=False,
                cwd=str(script_path.parent),
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_CONSOLE,
            )
        except OSError as e:
            logger.error(f"启动更新脚本失败: {e}")
            return None


class MsiUpdateStrategy(UpdateStrategy):
    """MSI 安装版更新策略。

    使用 update_msi.bat 脚本执行更新，确保：
    1. 等待旧进程完全退出
    2. 以管理员权限运行 MSI
    3. 正确处理安装结果
    """

    # 脚本模板资源路径
    UPDATE_SCRIPT_RESOURCE = ":/script/script/update_msi.bat"
    UPDATE_SCRIPT_FILENAME = "update_msi.bat"

    def prepare(self, package_path: Path, base_path: Path) -> Path:
        """验证 MSI 文件并复制到标准位置。

        将 MSI 复制到 runtime/tmp/NapCatQQ-Desktop.msi，
        以便脚本统一处理。
        """
        if not package_path.is_file():
            raise ValueError(f"MSI 文件不存在: {package_path}")

        if not package_path.suffix.lower() == ".msi":
            raise ValueError(f"不是有效的 MSI 文件: {package_path}")

        # 验证 MSI 文件大小（至少 1MB）
        min_size = 1024 * 1024  # 1MB
        if package_path.stat().st_size < min_size:
            raise ValueError(f"MSI 文件太小，可能已损坏: {package_path}")

        # 将 MSI 复制到标准位置（脚本期望的位置）
        tmp_dir = base_path / "runtime" / "tmp"
        tmp_dir.mkdir(parents=True, exist_ok=True)
        standard_msi_path = tmp_dir / "NapCatQQ-Desktop.msi"

        try:
            shutil.copy2(package_path, standard_msi_path)
            logger.info(f"MSI 文件已复制到: {standard_msi_path}")
        except OSError as e:
            raise ValueError(f"复制 MSI 文件失败: {e}")

        return standard_msi_path

    def get_update_script_path(self, staging_path: Path) -> Path:
        """返回 MSI 更新脚本路径。

        Args:
            staging_path: prepare 返回的 MSI 路径

        Returns:
            Path: 脚本路径（位于 runtime/tmp）
        """
        return staging_path.parent / self.UPDATE_SCRIPT_FILENAME

    def load_update_script(self) -> str:
        """读取 MSI 更新脚本模板。"""
        from PySide6.QtCore import QFile, QIODevice

        resource_file = QFile(self.UPDATE_SCRIPT_RESOURCE)
        if resource_file.open(QIODevice.OpenModeFlag.ReadOnly | QIODevice.OpenModeFlag.Text):
            try:
                # QByteArray to bytes conversion - runtime works but type checker complains
                return bytes(resource_file.readAll()).decode("utf-8")  # type: ignore
            finally:
                resource_file.close()

        # 回退到文件系统读取
        script_path = Path(__file__).resolve().parents[2] / "resource" / "script" / self.UPDATE_SCRIPT_FILENAME
        return script_path.read_text(encoding="utf-8")

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        """执行 MSI 升级安装（通过脚本）。

        脚本会处理：
        1. 等待旧进程退出（最多 60 秒，超时强制结束）
        2. 申请管理员权限（UAC）
        3. 执行 msiexec 升级安装
        4. 记录详细日志

        Args:
            staging_path: prepare 返回的 MSI 路径
            target_pid: 要等待退出的进程 ID

        Returns:
            subprocess.Popen | None: 更新脚本进程
        """
        msi_path = staging_path
        script_path = self.get_update_script_path(msi_path)

        # 写入更新脚本
        try:
            script_content = self.load_update_script()
            # 注入目标 PID
            if target_pid:
                script_content = self._inject_target_pid(script_content, target_pid)
            script_path.write_text(script_content, encoding="utf-8")
            logger.info(f"MSI 更新脚本已生成: {script_path}")
        except OSError as e:
            logger.error(f"写入 MSI 更新脚本失败: {e}")
            return None

        # 启动更新脚本（脚本内部会申请管理员权限）
        try:
            return subprocess.Popen(
                [str(script_path)],
                shell=False,
                cwd=str(script_path.parent),
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_CONSOLE,
            )
        except OSError as e:
            logger.error(f"启动 MSI 更新脚本失败: {e}")
            return None

    def _inject_target_pid(self, script_content: str, target_pid: int) -> str:
        """为脚本注入目标进程 PID。"""
        target_pid_line = f'set "target_pid={target_pid}"'
        script_lines = script_content.splitlines()

        for index, line in enumerate(script_lines):
            if line.strip().lower() == "setlocal enabledelayedexpansion":
                script_lines.insert(index + 1, target_pid_line)
                break
        else:
            script_lines.insert(0, target_pid_line)

        return "\n".join(script_lines) + "\n"


class UpdateManager:
    """统一更新管理器。

    根据安装类型自动选择更新策略。

    Usage:
        manager = UpdateManager(base_path)
        staging = manager.prepare_update(downloaded_package)
        process = manager.execute_update(staging, target_pid=os.getpid())
    """

    def __init__(self, base_path: Path | None = None):
        """初始化更新管理器。

        Args:
            base_path: 应用根目录，默认自动检测
        """
        if base_path is None:
            base_path = _get_default_base_path()

        self.base_path = base_path.resolve()
        self.install_type = detect_install_type(self.base_path)
        self._strategy = self._create_strategy()

        logger.info(f"更新管理器初始化: type={self.install_type.value}, path={self.base_path}")

    def _create_strategy(self) -> UpdateStrategy:
        """根据安装类型创建对应的更新策略。"""
        if self.install_type == InstallType.MSI:
            return MsiUpdateStrategy()
        else:
            # 便携版或未知类型都使用便携版策略
            return PortableUpdateStrategy()

    @property
    def install_type(self) -> InstallType:
        """当前安装类型。"""
        return self._install_type

    @install_type.setter
    def install_type(self, value: InstallType) -> None:
        self._install_type = value

    def get_update_filename(self, version: str) -> str:
        """获取当前安装类型对应的更新文件名。

        Args:
            version: 目标版本号（不含 v 前缀）

        Returns:
            str: 更新文件名
        """
        return get_update_file_pattern(self._install_type, version)

    def prepare_update(self, package_path: Path) -> Path:
        """准备更新包。

        Args:
            package_path: 下载的更新包路径

        Returns:
            Path: 准备好的更新入口路径
        """
        return self._strategy.prepare(package_path, self.base_path)

    def get_update_script_path(self, staging_path: Path) -> Path | None:
        """获取更新脚本路径（便携版）。"""
        return self._strategy.get_update_script_path(staging_path)

    def execute_update(self, staging_path: Path, target_pid: int | None = None) -> subprocess.Popen | None:
        """执行更新。

        Args:
            staging_path: prepare_update 返回的路径
            target_pid: 要等待退出的进程 ID（可选）

        Returns:
            subprocess.Popen | None: 更新进程
        """
        return self._strategy.execute_update(staging_path, target_pid)

    def is_msi_installation(self) -> bool:
        """是否为 MSI 安装版。"""
        return self._install_type == InstallType.MSI

    def is_portable_installation(self) -> bool:
        """是否为便携版。"""
        return self._install_type == InstallType.PORTABLE


# ========== 向后兼容的函数接口 ==========


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
    """验证并解压目录版更新包（向后兼容）。

    新代码建议使用 UpdateManager 类。

    Returns:
        Path: staging 中的应用目录。
    """
    strategy = PortableUpdateStrategy()
    return strategy.prepare(zip_path, base_path)


def inject_target_pid(script_content: str, target_pid: int) -> str:
    """为更新脚本注入目标进程 PID。"""
    target_pid_line = f'set "target_pid={target_pid}"'
    script_lines = script_content.splitlines()

    for index, line in enumerate(script_lines):
        if line.strip().lower() == "setlocal enabledelayedexpansion":
            script_lines.insert(index + 1, target_pid_line)
            break
    else:
        script_lines.insert(0, target_pid_line)

    return "\n".join(script_lines) + "\n"


def fetch_remote_update_script(script_url: str) -> str:
    """下载远端 updater 脚本，失败时自动尝试镜像。"""
    errors: list[str] = []
    for candidate_url in _iter_update_script_urls(script_url):
        try:
            with httpx.Client(timeout=8, follow_redirects=True) as client:
                response = client.get(candidate_url)
                response.raise_for_status()
                script_content = response.text.strip()
                if not script_content:
                    raise ValueError("脚本内容为空")
                return script_content + "\n"
        except (httpx.RequestError, httpx.HTTPStatusError, ValueError) as exc:
            errors.append(f"{candidate_url}: {exc}")

    raise ValueError("无法获取远端更新脚本: " + " | ".join(errors))


# ========== 内部辅助函数 ==========


def _get_default_base_path() -> Path:
    """获取默认应用根目录。"""
    import sys

    if getattr(sys, "frozen", False):
        return Path(sys.executable).parent
    else:
        return Path(__file__).parent.parent.parent.parent


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


def _iter_update_script_urls(script_url: str) -> list[str]:
    """生成远端更新脚本候选地址列表。"""
    candidate_urls = [script_url]
    candidate_urls.extend(f"{mirror.toString().rstrip('/')}/{script_url}" for mirror in Urls.MIRROR_SITE.value)
    return candidate_urls
