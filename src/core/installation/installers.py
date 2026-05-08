# -*- coding: utf-8 -*-
"""
## 安装逻辑
"""
# 标准库导入
import hashlib
import shutil
import subprocess
import zipfile
from pathlib import Path
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, QRunnable, Signal

# 项目内模块导入
from src.core.common.status import ButtonStatus, ProgressRingStatus
from src.core.installation.errors import NapCatHashMismatchError
from src.core.logging import LogSource, LogType, logger
from src.core.runtime.paths import PathFunc

if TYPE_CHECKING:
    from src.core.versioning.release_hash_service import ReleaseHashService


# 流式读取 chunk 大小; 4MB 在内存与系统调用次数之间取折中
_HASH_CHUNK_SIZE = 4 * 1024 * 1024


def verify_napcat_archive(
    *,
    version: str,
    archive_path: Path,
    hash_service: "ReleaseHashService",
) -> bool:
    """对 NapCat 安装包做 SHA512 完整性校验 (P5 安全收尾 F1.3).

    Args:
        version: 期望版本号, 接受 ``"v4.18.1"`` / ``"4.18.1"`` 两种形式
        archive_path: 待校验的本地 zip 路径
        hash_service: 已经 ``fetch()`` 过的 :class:`ReleaseHashService`

    Returns:
        - ``True``: 校验通过, archive 内容与上游 hash 一致, 调用方可继续解压
        - ``False``: 上游 release.json 中没有该版本的 hash 数据 (网络异常无缓存 /
          版本太新尚未发布 hash). 调用方应弹"二次确认"对话框让用户决定是否继续.
          archive 不会被删除.

    Raises:
        NapCatHashMismatchError: archive 的实际 SHA512 与上游期望不一致;
            archive 文件**已被删除**, 防止后续误用.
        FileNotFoundError: archive 路径不存在.
    """
    if not archive_path.exists():
        raise FileNotFoundError(f"待校验的 archive 不存在: {archive_path}")

    entry = hash_service.lookup(version)
    if entry is None:
        logger.warning(
            (
                f"verify_napcat_archive: 上游未提供 {version} 的 SHA512, 跳过校验; "
                "调用方应在 UI 层弹二次确认对话框"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return False

    expected = entry.shell_sha512
    actual = _compute_sha512(archive_path)
    if actual.lower() != expected.lower():
        logger.error(
            (
                "verify_napcat_archive: SHA512 不匹配, 已拒绝安装并删除 archive: "
                f"version={entry.version}, expected={expected}, actual={actual}, "
                f"archive={archive_path}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        try:
            archive_path.unlink()
        except OSError as exc:
            logger.warning(
                f"verify_napcat_archive: 删除非法 archive 失败 (忽略): {exc!r}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        raise NapCatHashMismatchError(
            version=entry.version,
            expected=expected,
            actual=actual,
            archive_path=str(archive_path),
        )

    logger.info(
        f"verify_napcat_archive: SHA512 校验通过 (version={entry.version}, archive={archive_path})",
        LogType.FILE_FUNC,
        LogSource.CORE,
    )
    return True


def _compute_sha512(path: Path) -> str:
    """流式计算 ``path`` 的 SHA512 hex digest."""
    hasher = hashlib.sha512()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(_HASH_CHUNK_SIZE)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


class InstallBase(QObject, QRunnable):
    """安装工具基类, 包含通用信号"""

    # 安装成功信号
    install_finish_signal = Signal()
    # 安装失败信号
    error_finish_signal = Signal()
    # 按钮模式切换
    button_toggle_signal = Signal(ButtonStatus)
    # 进度条模式切换
    progress_ring_toggle_signal = Signal(ProgressRingStatus)
    # 状态标签
    status_label_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """运行安装逻辑"""
        self.execute()

    def execute(self) -> None:
        """执行安装逻辑 (子类必须实现)"""
        raise NotImplementedError("Subclasses must implement this method")


class NapCatInstall(InstallBase):
    """NapCat 安装逻辑"""

    def __init__(self) -> None:
        super().__init__()
        self.zip_file_path = it(PathFunc).tmp_path / "NapCat.Shell.zip"
        self.install_path = it(PathFunc).napcat_path

    def execute(self) -> None:
        """安装逻辑"""
        try:
            logger.info(f"开始安装 NapCat: target={self.install_path}", LogType.FILE_FUNC, LogSource.CORE)
            self.status_label_signal.emit("正在安装 NapCat")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)
            self.ensure_install_path()
            # 移除 NapCat 文件夹下除了 config 和 log 文件夹外的所有文件
            self.remove_old_file()
            # 解压文件
            self.unzip_file()

        except Exception as e:
            self.status_label_signal.emit(self.tr("安装失败"))
            self.error_finish_signal.emit()
            logger.exception("安装 NapCat 失败", e, LogType.FILE_FUNC, LogSource.CORE)

    def ensure_install_path(self) -> None:
        """确保安装目录存在. """
        if self.install_path.exists():
            return

        logger.warning(
            f"NapCat 安装目录不存在，准备重新创建: target={self.install_path}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.install_path.mkdir(parents=True, exist_ok=True)

    def remove_old_file(self) -> None:
        """删除旧文件"""
        logger.info(f"开始删除 NapCat 旧文件: target={self.install_path}", LogType.FILE_FUNC, LogSource.CORE)
        self.status_label_signal.emit("正在删除旧文件")
        self.ensure_install_path()

        for item in self.install_path.iterdir():
            if item.is_dir() and item.name not in ["config", "log"]:
                shutil.rmtree(item)
            elif item.is_file():
                item.unlink()
        self.status_label_signal.emit("旧文件删除成功")
        logger.info("NapCat 旧文件删除完成", LogType.FILE_FUNC, LogSource.CORE)

    def unzip_file(self) -> None:
        """解压文件"""
        logger.info(
            f"开始解压 NapCat 安装包: source={self.zip_file_path}, target={self.install_path}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.status_label_signal.emit("正在解压文件")
        self.ensure_install_path()
        with zipfile.ZipFile(self.zip_file_path, "r") as zip_ref:
            zip_ref.extractall(self.install_path)
        self.zip_file_path.unlink()  # 移除安装包
        self.install_finish_signal.emit()
        logger.info("NapCat 安装完成", LogType.FILE_FUNC, LogSource.CORE)


class QQInstall(InstallBase):
    """QQ 安装逻辑"""

    def __init__(self, exe_path: str | Path) -> None:
        super().__init__()
        self.exe_path: Path = exe_path if isinstance(exe_path, Path) else Path(exe_path)

    def execute(self) -> None:
        """安装逻辑"""
        try:
            logger.info(f"开始安装 QQ: installer={self.exe_path}", LogType.FILE_FUNC, LogSource.CORE)
            self.status_label_signal.emit("正在安装 QQ")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

            # 启动 QQ 安装程序
            result = subprocess.run([str(self.exe_path), "/s"])
            if result.returncode == 0:
                self.install_finish_signal.emit()
                logger.info("QQ 安装完成", LogType.FILE_FUNC, LogSource.CORE)
            else:
                self.error_finish_signal.emit()
                logger.error(f"QQ 安装程序返回非零退出码: {result.returncode}", LogType.FILE_FUNC, LogSource.CORE)

            self.exe_path.unlink()  # 移除安装包

        except Exception as e:
            self.status_label_signal.emit(self.tr(f"安装失败: {e}"))
            self.error_finish_signal.emit()
            logger.exception("安装 QQ 失败", e, LogType.FILE_FUNC, LogSource.CORE)

