# -*- coding: utf-8 -*-

# 第三方库导入
from creart import it
from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.Core import timer
from src.Ui.BotListPage import BotListWidget
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import info_bar, error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Ui.common.message_box import AskBox
from src.Core.Utils.InstallFunc import NapCatInstall
from src.Core.NetworkFunc.Downloader import GithubDownloader


class NapCatPage(PageBase):
    """
    ## NapCat 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitNapCatPage")
        self.appCard.setName("NapCatQQ")
        self.appCard.setHyperLabelName(self.tr("仓库地址"))
        self.appCard.setHyperLabelUrl(Urls.NAPCATQQ_REPO.value)

        self.appCard.installButton.clicked.connect(self.downloadSlot)
        self.appCard.updateButton.clicked.connect(self.downloadSlot)
        self.appCard.openFolderButton.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(PathFunc().getNapCatPath()))
        )

    def updatePage(self) -> None:
        """
        ## 更新页面
        """
        if self.localVersion is None:
            # 如果没有本地版本则显示安装按钮
            self.appCard.switchButton(ButtonStatus.UNINSTALLED)
            return

        if self.remoteVersion is None:
            # 如果没有远程版本则不操作
            return

        if self.remoteVersion != self.localVersion:
            self.appCard.switchButton(ButtonStatus.UPDATE)
        else:
            self.appCard.switchButton(ButtonStatus.INSTALL)

        self.logCard.setLog(self.remoteLog)

    @Slot()
    def updateRemoteVersion(self) -> None:
        """
        ## 更新远程版本
        """
        self.remoteVersion = self.getVersion.remote_NapCat
        self.remoteLog = self.getVersion.updateLog_NapCat
        self.updatePage()

    @Slot()
    def updateLocalVersion(self) -> None:
        """
        ## 更新本地版本
        """
        self.localVersion = self.getVersion.local_NapCat

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载逻辑
        """
        if it(BotListWidget).getBotIsRun():
            # 项目内模块导入
            from src.Ui.MainWindow import MainWindow

            box = AskBox(
                self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), MainWindow()
            )
            box.yesButton.clicked.connect(it(BotListWidget).stopAllBot)
            box.yesButton.setText(self.tr("关闭全部"))

            if not box.exec():
                return

        info_bar(self.tr("正在下载 NapCat"))
        self.downloader = GithubDownloader(Urls.NAPCATQQ_DOWNLOAD.value)
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
        self.installer = NapCatInstall()
        self.installer.statusLabel.connect(self.appCard.setStatusText)
        self.installer.errorFinsh.connect(self.errorFinshSlot)
        self.installer.buttonToggle.connect(self.appCard.switchButton)
        self.installer.progressRingToggle.connect(self.appCard.switchProgressRing)
        self.installer.installFinish.connect(self.installFinshSlot)
        self.installer.start()

    @Slot()
    def installFinshSlot(self) -> None:
        """
        ## 安装结束逻辑
        """
        success_bar(self.tr("安装成功 !"))
        self.appCard.switchButton(ButtonStatus.INSTALL)

    @Slot()
    def errorFinshSlot(self) -> None:
        """
        ## 错误结束逻辑
        """
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.updatePage()  # 刷新一次页面
