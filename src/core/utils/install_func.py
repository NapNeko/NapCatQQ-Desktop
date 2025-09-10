# -*- coding: utf-8 -*-
"""
## 安装逻辑
"""
# 标准库导入
import shutil
import subprocess
import zipfile
from pathlib import Path

from PySide6.QtCore import QThread, QUrl, Signal

# 项目内模块导入
from src.core.utils.path_func import PathFunc
from src.ui.page.unit_page.status import ButtonStatus, ProgressRingStatus


class NapCatInstall(QThread):
    """NapCat 安装逻辑"""

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
        super().__init__()
        self.zip_file_path = PathFunc().tmp_path / "NapCat.Shell.zip"
        self.install_path = PathFunc().napcat_path

    def run(self) -> None:
        """安装逻辑"""
        try:
            self.status_label_signal.emit("正在安装 NapCat")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)
            # 移除 NapCat 文件夹下除了 config 和 log 文件夹外的所有文件
            self.remove_old_file()
            # 解压文件
            self.unzip_file()
            # 写入JS
            self.write_js_file()

        except Exception as e:
            self.status_label_signal.emit(self.tr("安装失败"))
            self.error_finish_signal.emit()

    def remove_old_file(self) -> None:
        """删除旧文件"""
        self.status_label_signal.emit("正在删除旧文件")
        for item in self.install_path.iterdir():
            if item.is_dir() and item.name not in ["config", "log"]:
                shutil.rmtree(item)
            elif item.is_file():
                item.unlink()
        self.status_label_signal.emit("旧文件删除成功")

    def unzip_file(self) -> None:
        """解压文件"""
        self.status_label_signal.emit("正在解压文件")
        with zipfile.ZipFile(self.zip_file_path, "r") as zip_ref:
            zip_ref.extractall(self.install_path)
        self.zip_file_path.unlink()  # 移除安装包
        self.install_finish_signal.emit()

    def write_js_file(self) -> None:
        """写入 JS 文件"""
        with open(str(self.install_path / "loadNapCat.js"), "w") as file:
            file.write(
                "(async () => {await import("
                f"'{QUrl.fromLocalFile(str(self.install_path / 'napcat.mjs')).url()}'"
                ")})()"
            )


class QQInstall(QThread):
    """QQ 安装逻辑"""

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

    def __init__(self, exe_path: str | Path) -> None:
        super().__init__()
        self.exe_path: Path = exe_path if isinstance(exe_path, Path) else Path(exe_path)

    def run(self) -> None:
        """安装逻辑"""
        try:
            self.status_label_signal.emit("正在安装 QQ")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

            # 启动 QQ 安装程序
            if subprocess.run([str(self.exe_path), "/s"]).returncode == 0:
                self.install_finish_signal.emit()
            else:
                self.error_finish_signal.emit()

            self.exe_path.unlink()  # 移除安装包

        except Exception as e:
            self.status_label_signal.emit(self.tr(f"安装失败: {e}"))
            self.error_finish_signal.emit()
