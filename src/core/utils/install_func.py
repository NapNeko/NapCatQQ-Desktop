# -*- coding: utf-8 -*-
"""
## 安装逻辑
"""
# 标准库导入
import shutil
import subprocess
import zipfile
from pathlib import Path

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, QRunnable, Signal

# 项目内模块导入
from src.core.common.status import ButtonStatus, ProgressRingStatus
from src.core.utils.logger import LogSource, LogType, logger
from src.core.utils.path_func import PathFunc


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
            # 移除 NapCat 文件夹下除了 config 和 log 文件夹外的所有文件
            self.remove_old_file()
            # 解压文件
            self.unzip_file()

        except Exception as e:
            self.status_label_signal.emit(self.tr("安装失败"))
            self.error_finish_signal.emit()
            logger.exception("安装 NapCat 失败", e, LogType.FILE_FUNC, LogSource.CORE)

    def remove_old_file(self) -> None:
        """删除旧文件"""
        logger.info(f"开始删除 NapCat 旧文件: target={self.install_path}", LogType.FILE_FUNC, LogSource.CORE)
        self.status_label_signal.emit("正在删除旧文件")
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
