# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path

# 第三方库导入
from creart import it
from PySide6.QtCore import Slot

# 项目内模块导入
from src.Ui.BotListPage import BotListWidget
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import info_bar, error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus, ProgressRingStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Ui.common.message_box import AskBox
from src.Core.Utils.InstallFunc import DLCInstall
from src.Core.NetworkFunc.Downloader import GithubDownloader

DESCRIPTION_TEXT = """
### 这是什么?
NapCat DLC当前支持多个功能，旨在提升用户在群聊和文件管理中的体验。用户可以在群聊中设置个性化的头衔，发送提醒消息（群poke）以增加互动。同时，系统提供独立的 Rkey 获取方式，以便于在不同场景中使用。此外，用户能够获取陌生人的在线状态，方便了解他们的活动情况。合并多条消息并伪造转发功能，能够使信息传递更为流畅。对于文件管理，用户可以获取文件的直接下载链接，简化文件获取流程。最后，支持 Markdown 语法的文本格式化，使得信息的表达更加清晰和美观。这些功能共同构成了一个高效、便捷的用户体验

### 当前支持版本

1. **版本**: Windows Amd64 28418-28788
2. **版本**: Linux Amd64 28498-28788

### 扩展进度

1. **功能**: 设置群头衔
2. **功能**: 发送群poke
3. **功能**: 独立Rkey获取
4. **功能**: 陌生人状态获取
5. **功能**: 伪造合并转发
6. **功能**: 文件直链获取
7. **功能**: markdown
"""


class DLCPage(PageBase):
    """
    ## 扩展页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitDLCPage")

        # 设置 appCard
        self.appCard.setName("NapCat DLC")
        self.appCard.setHyperLabelName(self.tr("仓库地址"))
        self.appCard.setHyperLabelUrl(Urls.NAPCATQQ_REPO.value)
        self.appCard.installButton.setText(self.tr("安装"))
        self.appCard.updateButton.setText(self.tr("重新安装"))

        # 设置 logCard
        self.logCard.setTitle(self.tr("介绍"))
        self.logCard.setUrl(Urls.NAPCATQQ_DLC_DOCUMENT.value.url())
        self.logCard.setLog(DESCRIPTION_TEXT)

        # 信号链接
        self.appCard.installButton.clicked.connect(self.downloadSlot)
        self.appCard.updateButton.clicked.connect(self.downloadSlot)

        # 调用方法
        self.setButton()

    @staticmethod
    def isInstall():
        """
        ## 是否安装
        """
        return Path(it(PathFunc).dlc_path / Urls.NAPCATQQ_DLC_DOWNLOAD.value.fileName()).exists()

    def setButton(self):
        """
        ## 设置按钮
        """
        self.appCard.switchProgressRing(ProgressRingStatus.NONE)
        if Path(it(PathFunc).dlc_path / Urls.NAPCATQQ_DLC_DOWNLOAD.value.fileName()).exists():
            self.appCard.switchButton(ButtonStatus.UPDATE)
        else:
            self.appCard.switchButton(ButtonStatus.UNINSTALLED)

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载逻辑
        """
        if it(BotListWidget).getBotIsRun():
            # 项目内模块导入
            from src.Ui.MainWindow import MainWindow

            box = AskBox(
                self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), it(MainWindow)
            )
            box.yesButton.clicked.connect(it(BotListWidget).stopAllBot)
            box.yesButton.setText(self.tr("关闭全部"))

            if not box.exec():
                return

        info_bar("开始下载 DLC")
        self.downloader = GithubDownloader(Urls.NAPCATQQ_DLC_DOWNLOAD.value)
        self.downloader.downloadProgress.connect(self.appCard.setProgressRingValue)
        self.downloader.statusLabel.connect(self.appCard.setStatusText)
        self.downloader.errorFinsh.connect(self.errorFinshSlot)
        self.downloader.progressRingToggle.connect(self.appCard.switchProgressRing)
        self.downloader.downloadFinish.connect(self.installSlot)
        self.downloader.start()

    def installSlot(self) -> None:
        """
        ## 安装逻辑
        """
        success_bar(self.tr("下载成功, 正在安装..."))
        self.installer = DLCInstall()
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
        self.appCard.switchButton(ButtonStatus.UPDATE)

    @Slot()
    def errorFinshSlot(self) -> None:
        """
        ## 错误结束逻辑
        """
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.setButton()
