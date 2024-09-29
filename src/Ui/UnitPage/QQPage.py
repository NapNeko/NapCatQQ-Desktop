# -*- coding: utf-8 -*-
# 标准库导入
import winreg

# 第三方库导入
from creart import it
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.Core import timer
from src.Ui.UnitPage.Base import PageBase
from src.Ui.common.info_bar import error_bar, success_bar
from src.Ui.UnitPage.status import ButtonStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Core.Utils.GetVersion import GetVersion
from src.Ui.common.message_box import FolderBox
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
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).getQQPath()))
        )

        # 启动计时器
        self.updatePage()

    @timer(900_000)
    def updatePage(self) -> None:
        """
        ## 调用方法更新页面内容
        """
        self.checkStatus()

    def checkStatus(self) -> None:
        """
        ## 检查是否有更新, 发现更新改变按钮状态
        """
        if (locaL_ver := it(GetVersion).getLocalQQVersion()) is None:
            # 如果没有本地版本则显示安装按钮
            self.appCard.switchButton(ButtonStatus.UNINSTALLED)
            return

        if (remote_ver := it(GetVersion).getRemoteQQVersion()) is None:
            # 如果拉取不到远程版本
            error_bar(self.tr("拉取远程版本时发生错误, 详情查看 设置 > Log"))
            return

        if locaL_ver != remote_ver:
            # 判断版本是否相等, 否则设置为更新状态
            self.appCard.switchButton(ButtonStatus.UPDATE)
        else:
            self.appCard.switchButton(ButtonStatus.INSTALL)

    @Slot()
    def downloadSlot(self) -> None:
        """
        ## 下载按钮槽函数
        """
        if (url := it(GetVersion).getQQDownloadUrl()) is None:
            error_bar(self.tr("获取下载地址时发生错误, 详情查看 设置 > Log"))
            return
        self.file_path = it(PathFunc).tmp_path / QUrl(url).fileName()
        self.downloader = QQDownloader(QUrl(url))
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
        box = FolderBox(self.tr("选择安装路径"), it(MainWindow))

        if box.exec():
            # 如果点击了确定按钮

            # 修改注册表, 让安装程序读取注册表按照路径安装
            key = winreg.CreateKey(winreg.HKEY_CURRENT_USER, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
            winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, box.getValue())
            winreg.CloseKey(key)

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
        self.updatePage()  # 刷新一次页面

    @Slot()
    def errorFinshSlot(self) -> None:
        """
        ## 错误结束逻辑
        """
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.updatePage()  # 刷新一次页面
