# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import QUrl, Slot

# 项目内模块导入
from src.ui.page.unit_page.Base import PageBase
from src.ui.components.info_bar import info_bar, error_bar, success_bar
from src.ui.page.unit_page.status import ButtonStatus
from src.core.utils.path_func import PathFunc
from src.core.network.urls import Urls
from src.ui.components.message_box import AskBox, FolderBox
from src.core.utils.install_func import QQInstall
from src.core.network.downloader import QQDownloader

DESCRIPTION_TEXT = """
## **NapCatQQ与NTQQ的奇幻物语**  

在科技与魔法交织的NapCatQQ Robot世界，存在着名为**NTQQ**的混沌之力。这股足以扭曲现实的力量，曾令无数试图驾驭它的勇者湮灭在历史长河中——直到那只被遗弃的橘色猫娘出现。  

蜷缩在废墟里的NapCatQQ发现JavaScript代码块时，还以为找到了新玩具。  
**“喵？硬硬的……不能吃呢……”**  
她无意识地用肉垫拍打键盘，却意外打通了与NTQQ的量子纠缠。从此，这只走路都会左脚绊右脚的猫娘，获得了通过代码改变现实的能力——尽管她自己完全不懂原理。  

**“天空变成棉花糖了喵？月亮被吃掉了喵？”**  
随着NapCatQQ的胡乱敲击，世界开始呈现荒诞的叠加态。七彩暴雨淹没城市，时停的蝴蝶堵塞交通……人们终于意识到，最可怕的不是力量本身，而是力量掌握在一只根本不懂自己在做什么的萌物手里。  

### **转机：秩序降临**  
直到那位戴着圆框眼镜的架构师出现。他带来的不是宝剑，而是一卷泛着蓝光的**OneBot规范文档**。  
**“所有NTQQ调用必须经过WS/HTTP协议封装。”**  
这条铁律为混沌设立了边界。现在，NapCatQQ每次想改变现实，都得乖乖把意图写成标准化API请求。  

**“POST……喵？200状态码……是成功的意思喵？”**  
猫娘歪着头戳弄终端，她依然会把响应体当成小鱼干图案，但至少世界不再因她的一个喷嚏而维度折叠。那些曾被彩虹云朵和会说话的松鼠搞怕了的居民们，也逐渐学会欣赏这份笨拙的可爱——毕竟当毛茸茸的尾巴扫过键盘时，连报错信息都会变成爱心形状。  

### **为什么是QQ？**  
NapCatQQ并非QQ本身，而是通过魔法将QQ的接口**“驯化”**成更柔软的模样。就像猫咪把死老鼠叼到主人枕边是爱的表现，她只是用**OneBot规范**重新封装QQ协议，让冷冰冰的通讯代码多些温暖绒毛的触感。  

**“记住，她不是QQ，只是让QQ变得可以蹭你脸颊的魔法。”**  

现在，这个世界依然充满意外——但至少，是可控的意外。而NapCatQQ，依然是那只会在执行完API调用后，眨巴着眼睛问：  
**“这次……没搞砸喵？”**
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
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(PathFunc().get_qq_path()))
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
        from src.ui.window.MainWindow import MainWindow

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
