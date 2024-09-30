# -*- coding: utf-8 -*-

# 第三方库导入
from creart import it
from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.Core import timer
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import info_bar, error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Core.Utils.GetVersion import GetVersion
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
        if (local_ver := it(GetVersion).getLocalNapCatVersion()) is None:
            # 如果没有本地版本则显示安装按钮
            self.appCard.switchButton(ButtonStatus.UNINSTALLED)
            return

        if (remote_ver := it(GetVersion).getRemoteNapCatVersion()) is None:
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
        if (log := it(GetVersion).getRemoteNapCatUpdateLog()) is None:
            error_bar(self.tr("拉取更新日志时发生错误, 详情查看 设置 > Log"))
            return

        self.logCard.setLog(log)

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载逻辑
        """
        if self.checkBotIsRun():
            # 项目内模块导入
            from src.Ui.MainWindow import MainWindow

            box = AskBox(self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), it(MainWindow))
            box.yesButton.clicked.connect(self.closeAllBot)
            box.yesButton.setText(self.tr("关闭全部"))

            if not box.exec():
                return

        info_bar(self.tr("正在下载 NapCat"))
        self.downloader = GithubDownloader(Urls.NAPCAT_DOWNLOAD.value)
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
        self.updatePage()  # 刷新一次页面

    @Slot()
    def errorFinshSlot(self) -> None:
        """
        ## 错误结束逻辑
        """
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.updatePage()  # 刷新一次页面

    @staticmethod
    def checkBotIsRun() -> bool:
        """
        ## 检查 NapCat 是否有在运行
        """
        # 项目内模块导入
        from src.Ui.BotListPage import BotListWidget

        # 遍历所有卡片
        for card in it(BotListWidget).botList.botCardList:
            if card.botWidget is None:
                continue

            if card.botWidget.isRun:
                return True

        return False

    @staticmethod
    def closeAllBot() -> None:
        """
        ## 关闭所有bot
        """
        # 项目内模块导入
        from src.Ui.BotListPage import BotListWidget

        # 遍历所有卡片
        for card in it(BotListWidget).botList.botCardList:
            if card.botWidget is None:
                continue

            if card.botWidget.isRun:
                card.botWidget.stopButtonSlot()
