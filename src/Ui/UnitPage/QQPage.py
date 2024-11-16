# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import info_bar, error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Ui.common.message_box import AskBox, FolderBox
from src.Core.Utils.InstallFunc import QQInstall
from src.Core.NetworkFunc.Downloader import QQDownloader

DESCRIPTION_TEXT = """
## 为什么要 QQ?

NapCat 通过魔法的手段获得了 QQ 的发送消息、接收消息等接口，为了方便使用，猫猫框架将通过一种名为 [OneBot](https://11.onebot.dev) 的约定将你的 HTTP / WebSocket 请求按照规范读取，再去调用猫猫框架所获得的QQ发送接口之类的接口。

所以 NapCat 只是让 QQ 以一种更加优雅的方式运行在你的服务器上，而不是 QQ 本身, 但是本质上还是离不开 QQ 的。
"""


class QQPage(PageBase):
    """
    ## QQ 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitQQPage")
        self.url = None

        # 设置 appCard
        self.appCard.setIcon(":/Icon/image/Icon/black/QQ.svg")
        self.appCard.setName("QQ")
        self.appCard.setHyperLabelName(self.tr("官网地址"))
        self.appCard.setHyperLabelUrl(Urls.QQ_OFFICIAL_WEBSITE.value)

        # 设置 logCard
        self.logCard.setUrl(Urls.QQ_OFFICIAL_WEBSITE.value.url())
        self.logCard.setTitle(self.tr("描述"))
        self.logCard.setLog(DESCRIPTION_TEXT)

        # 链接信号
        self.appCard.installButton.clicked.connect(self.downloadSlot)
        self.appCard.updateButton.clicked.connect(self.downloadSlot)
        self.appCard.openFolderButton.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(PathFunc().getQQPath()))
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

    @Slot()
    def updateRemoteVersion(self) -> None:
        """
        ## 更新远程版本
        """
        self.remoteVersion = self.getVersion.remote_QQ
        self.url = QUrl(self.getVersion.download_qq_url)
        self.updatePage()

    @Slot()
    def updateLocalVersion(self) -> None:
        """
        ## 更新本地版本
        """
        self.localVersion = self.getVersion.local_QQ

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载按钮槽函数
        """
        if self.url is None:
            error_bar(self.tr("QQ下载链接为空"))
            return
        self.file_path = PathFunc().tmp_path / self.url.fileName()
        self.downloader = QQDownloader(self.url)
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
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        success_bar(self.tr("下载成功, 正在安装..."))

        # 创建询问弹出
        folder_box = FolderBox(self.tr("选择安装路径"), MainWindow())

        if not folder_box.exec():
            # 如果没有点击确定按钮
            self.file_path.unlink()
            info_bar(self.tr("取消安装"))
            return

        # 修改注册表, 让安装程序读取注册表按照路径安装
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.getValue().replace("/", "\\"))
        winreg.CloseKey(key)

        # 检查是否存在 dbghelp.dll 文件, 否则会导致安装失败
        if Path(Path(folder_box.getValue()) / "dbghelp.dll").exists():
            rm_dll_box = AskBox(
                self.tr("检测到修补文件"), self.tr("您需要删除 dbghelp.dll 才能正确安装QQ"), MainWindow()
            )
            rm_dll_box.yesButton.setText(self.tr("删除"))
            if rm_dll_box.exec():
                # 用户点击了删除
                Path(Path(folder_box.getValue()) / "dbghelp.dll").unlink()
            else:
                self.file_path.unlink()
                info_bar(self.tr("取消安装"))
                return

        # 开始安装
        self.installer = QQInstall(self.file_path)
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
