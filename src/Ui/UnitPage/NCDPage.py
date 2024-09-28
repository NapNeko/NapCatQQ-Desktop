# -*- coding: utf-8 -*-
# 标准库导入
import sys
import subprocess

# 第三方库导入
from creart import it
from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.Core import timer
from src.Core.Config import cfg
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import info_bar, error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Core.Utils.GetVersion import GetVersion


class NCDPage(PageBase):
    """
    ## NapCat Desktop 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitNCDPage")
        self.appCard.setName("NapCat Desktop")
        self.appCard.setHyperLabelName(self.tr("仓库地址"))
        self.appCard.setHyperLabelUrl(Urls.NCD_REPO.value)

        self.appCard.installButton.clicked.connect(self.downloadSlot)
        self.appCard.updateButton.clicked.connect(self.downloadSlot)
        self.appCard.openFolderButton.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).getNapCatPath()))
        )
        # 启动计时器
        self.updatePage()

    @timer(900_000)
    def updatePage(self) -> None:
        """
        ## 调用方法更新页面内容
            - 自动更新频率: 15分钟更新一次
        """
        self.checkStatus()
        self.getNewUpdateLog()

    def checkStatus(self) -> None:
        """
        ## 检查是否有更新, 发现更新改变按钮状态
        """
        if (local_ver := cfg.get(cfg.NCDVersion)) is None:
            # 如果没有本地版本则显示安装按钮
            self.appCard.switchButton(ButtonStatus.UNINSTALLED)
            return

        if (remote_ver := it(GetVersion).getRemoteNCDVersion()) is None:
            # 如果拉取不到远程版本
            error_bar(self.tr("拉取远程版本时发生错误, 详情查看 设置 > Log"))
            return

        if local_ver != remote_ver:
            # 判断版本是否相等, 否则设置为更新状态
            self.appCard.switchButton(ButtonStatus.UPDATE)
        else:
            self.appCard.switchButton(ButtonStatus.INSTALL)

    def getNewUpdateLog(self) -> None:
        """
        ## 拉取最新更新日志到卡片
        """
        if (log := it(GetVersion).getRemoteNCDUpdateLog()) is None:
            error_bar(self.tr("拉取更新日志时发生错误, 详情查看 设置 > Log"))
            return

        self.logCard.setLog(log)

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载逻辑
        """
        info_bar(self.tr("正在下载 NapCat Desktop"))
        self.downloader = GithubDownloader(Urls.NCD_DOWNLOAD.value)
        self.downloader.downloadProgress.connect(self.appCard.setProgressRingValue)
        self.downloader.downloadFinish.connect(self.installSlot)
        self.downloader.statusLabel.connect(self.appCard.setStatusText)
        self.downloader.errorFinsh.connect(self.errorFinshSlot)
        self.downloader.buttonToggle.connect(self.appCard.switchButton)
        self.downloader.progressRingToggle.connect(self.appCard.switchProgressRing)
        self.downloader.start()

    @Slot()
    def installSlot(self) -> None:
        """
        ## 安装逻辑
        """
        success_bar(self.tr("下载成功, 正在安装..."))

        # 写入安装脚本
        bat_content = "\n".join(
            [
                "@echo off",
                "setlocal",
                "",
                "rem 定义应用程序路径",
                'set "app_name=NapCat-Desktop.exe"',
                'set "current_app_path=%~dp0%app_name%"',
                'set "new_app_path=%~dp0tmp\\%app_name%"',
                "",
                "rem 等待旧版进程退出，如果其还在运行的话",
                ":wait_for_exit",
                'tasklist /FI "IMAGENAME eq %app_name%" 2>NUL | find /I /N "%app_name%">NUL',
                'if "%ERRORLEVEL%"=="0" (',
                "    echo Waiting for the application to exit...",
                "    timeout /T 1 /NOBREAK > NUL",
                "    goto wait_for_exit",
                ")",
                "",
                "rem 删除旧版本",
                'if exist "%current_app_path%" (',
                '    del "%current_app_path%"',
                ")",
                "",
                "rem 移动新版到应用程序目录",
                'move "%new_app_path%" "%current_app_path%"',
                "",
                "rem 启动新版本应用程序",
                'start "" "%current_app_path%"',
                "",
                "rem 删除自身",
                'del "%~f0"',
                "endlocal",
            ]
        )
        with open(str(it(PathFunc).base_path / "update.bat"), "w", encoding="utf-8") as file:
            file.write(bat_content)

        # 创建进程
        subprocess.Popen([str(it(PathFunc).base_path / "update.bat")], shell=True)

        # 退出程序
        sys.exit()

    @Slot()
    def installFinshSlot(self) -> None:
        """
        ## 安装结束逻辑
        """
        success_bar(self.tr("安装成功 !"))
        self.updatePage()  # 刷新一次页面

    @Slot()
    def errorFinshSlot(self) -> None:
        """
        ## 错误结束逻辑
        """
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.updatePage()  # 刷新一次页面
