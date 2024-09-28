# -*- coding: utf-8 -*-
"""
## 安装逻辑
"""
# 标准库导入
import shutil
import zipfile

# 第三方库导入
from creart import it
from loguru import logger
from PySide6.QtCore import QUrl, Signal, QThread

# 项目内模块导入
from src.Ui.UnitPage.status import ButtonStatus, ProgressRingStatus
from src.Core.Utils.PathFunc import PathFunc


class NapCatInstall(QThread):
    """NapCat 安装逻辑"""

    # 安装成功信号
    installFinish = Signal()
    # 安装失败信号
    errorFinsh = Signal()
    # 按钮模式切换
    buttonToggle = Signal(ButtonStatus)
    # 进度条模式切换
    progressRingToggle = Signal(ProgressRingStatus)
    # 状态标签
    statusLabel = Signal(str)

    def __init__(self) -> None:
        super().__init__()
        self.zip_file_path = it(PathFunc).tmp_path / "NapCat.Shell.zip"
        self.install_path = it(PathFunc).getNapCatPath()

    def run(self) -> None:
        """
        ## 安装逻辑
        """
        try:
            logger.info(f"{'-' * 10} 开始安装 NapCat ~ {'-' * 10}")
            self.statusLabel.emit("正在安装 NapCat")
            self.progressRingToggle.emit(ProgressRingStatus.INDETERMINATE)
            # 移除 NapCat 文件夹下除了 config 和 log 文件夹外的所有文件
            self.rmOldFile()
            # 解压文件
            self.unzipFile()
            # 写入JS
            self.writeJsFile()
            logger.info(f"{'-' * 10} 安装 NapCat 完成 ~ {'-' * 10}")

        except Exception as e:
            logger.error(f"安装 NapCat 时引发 {type(e).__name__}: {e}")
            self.statusLabel.emit(self.tr("安装失败"))
            self.errorFinsh.emit()

    def rmOldFile(self) -> None:
        """
        ## 删除旧文件
        """
        self.statusLabel.emit("正在删除旧文件")
        for item in self.install_path.iterdir():
            if item.is_dir() and item.name not in ["config", "log"]:
                logger.info(f"删除文件夹: {item}")
                shutil.rmtree(item)
            elif item.is_file():
                logger.info(f"删除文件: {item}")
                item.unlink()
        self.statusLabel.emit("旧文件删除成功")

    def unzipFile(self) -> None:
        """
        ## 解压文件
        """
        self.statusLabel.emit("正在解压文件")
        with zipfile.ZipFile(self.zip_file_path, "r") as zip_ref:
            zip_ref.extractall(self.install_path)
        self.zip_file_path.unlink()  # 移除安装包
        self.installFinish.emit()

    def writeJsFile(self) -> None:
        """
        ## 写入 JS 文件
        """
        with open(str(self.install_path / "loadNapCat.js"), "w") as file:
            file.write(
                "(async () => {await import("
                f"'{QUrl.fromLocalFile(str(self.install_path / 'napcat.mjs')).url()}'"
                ")})()"
            )