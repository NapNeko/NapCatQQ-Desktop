# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path
from typing import TYPE_CHECKING

from PySide6.QtCore import QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.get_version import VersionData
from src.core.utils.install_func import QQInstall
from src.core.utils.path_func import PathFunc
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox, FolderBox
from src.ui.page.unit_page.base import PageBase
from src.ui.page.unit_page.status import ButtonStatus

if TYPE_CHECKING:
    # 项目内模块导入
    from src.core.network.downloader import QQDownloader

DESCRIPTION_TEXT = """
## **NapCatQQ与NTQQ的奇幻物语**  

在科技与魔法交织的NapCatQQ Robot世界, 存在着名为**NTQQ**的混沌之力。这股足以扭曲现实的力量, 曾令无数试图驾驭它的勇者湮灭在历史长河中——直到那只被遗弃的橘色猫娘出现。  

蜷缩在废墟里的NapCatQQ发现JavaScript代码块时, 还以为找到了新玩具。  
**"喵? 硬硬的……不能吃呢……"**  
她无意识地用肉垫拍打键盘, 却意外打通了与NTQQ的量子纠缠。从此, 这只走路都会左脚绊右脚的猫娘, 获得了通过代码改变现实的能力——尽管她自己完全不懂原理。  

**"天空变成棉花糖了喵? 月亮被吃掉了喵? "**  
随着NapCatQQ的胡乱敲击, 世界开始呈现荒诞的叠加态。七彩暴雨淹没城市, 时停的蝴蝶堵塞交通……人们终于意识到, 最可怕的不是力量本身, 而是力量掌握在一只根本不懂自己在做什么的萌物手里。  

### **转机：秩序降临**  
直到那位戴着圆框眼镜的架构师出现。他带来的不是宝剑, 而是一卷泛着蓝光的**OneBot规范文档**。  
**"所有NTQQ调用必须经过WS/HTTP协议封装。"**  
这条铁律为混沌设立了边界。现在, NapCatQQ每次想改变现实, 都得乖乖把意图写成标准化API请求。  

**"POST……喵? 200状态码……是成功的意思喵? "**  
猫娘歪着头戳弄终端, 她依然会把响应体当成小鱼干图案, 但至少世界不再因她的一个喷嚏而维度折叠。那些曾被彩虹云朵和会说话的松鼠搞怕了的居民们, 也逐渐学会欣赏这份笨拙的可爱——毕竟当毛茸茸的尾巴扫过键盘时, 连报错信息都会变成爱心形状。  

### **为什么是QQ? **  
NapCatQQ并非QQ本身, 而是通过魔法将QQ的接口**"驯化"**成更柔软的模样。就像猫咪把死老鼠叼到主人枕边是爱的表现, 她只是用**OneBot规范**重新封装QQ协议, 让冷冰冰的通讯代码多些温暖绒毛的触感。  

**"记住, 她不是QQ, 只是让QQ变得可以蹭你脸颊的魔法。"**  

现在, 这个世界依然充满意外——但至少, 是可控的意外。而NapCatQQ, 依然是那只会在执行完API调用后, 眨巴着眼睛问：  
**"这次……没搞砸喵? "**
"""


class QQPage(PageBase):
    """QQ 安装与更新页面, 负责QQ的下载、安装和版本管理"""

    def __init__(self, parent) -> None:
        """初始化QQ页面

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UnitQQPage")
        self.url: QUrl | None = None
        self.file_path: Path | None = None
        self.downloader: "QQDownloader" | None = None
        self.installer: QQInstall | None = None

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self) -> None:
        """设置界面控件"""
        # 设置 appCard
        self.app_card.set_icon(NapCatDesktopIcon.QQ.path())
        self.app_card.set_name("QQ")
        self.app_card.set_hyper_label_name(self.tr("官网地址"))
        self.app_card.set_hyper_label_url(Urls.QQ_OFFICIAL_WEBSITE.value)

        # 设置 logCard
        self.log_card.set_url(Urls.QQ_OFFICIAL_WEBSITE.value.url())
        self.log_card.setTitle(self.tr("描述"))
        self.log_card.setLog(DESCRIPTION_TEXT)

    def _connect_signals(self) -> None:
        """连接信号与槽"""
        self.app_card.install_button.clicked.connect(self.on_download)
        self.app_card.update_button.clicked.connect(self.on_download)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(PathFunc().get_qq_path()))
        )

    def update_page(self) -> None:
        """根据本地和远程版本信息更新页面状态"""
        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            return

        if self.remote_version is None:
            # 如果没有远程版本则提示错误
            error_bar(self.tr("无法获取QQ版本信息, 请检查网络连接"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

    # ==================== 槽函数 ====================
    @Slot()
    def on_update_remote_version(self, version_data: VersionData) -> None:
        """更新远程版本信息"""
        if version_data.qq_version is None or version_data.qq_download_url is None:
            self.remote_version = None
            self.url = None
        else:
            self.remote_version = version_data.qq_version
            self.url = QUrl(version_data.qq_download_url)

        self.update_page()

    @Slot()
    def on_update_local_version(self, version_data: VersionData) -> None:
        """更新本地版本信息"""
        if version_data.qq_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.qq_version

    @Slot()
    def on_download(self) -> None:
        """处理下载按钮点击事件, 开始下载QQ安装包"""
        if self.url is None:
            error_bar(self.tr("QQ下载链接为空"))
            return

        # 项目内模块导入
        from src.core.network.downloader import QQDownloader

        self.file_path = PathFunc().tmp_path / self.url.fileName()
        downloader = QQDownloader(self.url)
        downloader.download_progress_signal.connect(self.app_card.set_progress_ring_value)
        downloader.download_finish_signal.connect(self.on_install)
        downloader.status_label_signal.connect(self.app_card.set_status_text)
        downloader.error_finsh_signal.connect(self.on_error_finsh)
        downloader.button_toggle_signal.connect(self.app_card.switch_button)
        downloader.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)

        QThreadPool.globalInstance().start(downloader)

    @Slot()
    def on_install(self) -> None:
        """处理下载完成后的安装逻辑"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        success_bar(self.tr("下载成功, 正在安装..."))

        # 创建询问弹出框
        folder_box = FolderBox(self.tr("选择安装路径"), MainWindow())

        if not folder_box.exec():
            # 如果没有点击确定按钮
            self.file_path.unlink()
            info_bar(self.tr("取消安装"))
            return

        # 修改注册表, 设置安装路径
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.get_value().replace("/", "\\"))
        winreg.CloseKey(key)

        # 检查是否存在 dbghelp.dll 文件, 否则会导致安装失败
        if Path(Path(folder_box.get_value()) / "dbghelp.dll").exists():
            rm_dll_box = AskBox(
                self.tr("检测到修补文件"), self.tr("您需要删除 dbghelp.dll 才能正确安装QQ"), MainWindow()
            )
            rm_dll_box.yesButton.setText(self.tr("删除"))
            if rm_dll_box.exec():
                # 用户点击了删除
                Path(Path(folder_box.get_value()) / "dbghelp.dll").unlink()
            else:
                self.file_path.unlink()
                info_bar(self.tr("取消安装"))
                return

        # 开始安装
        installer = QQInstall(self.file_path)
        installer.status_label_signal.connect(self.app_card.set_status_text)
        installer.error_finish_signal.connect(self.on_error_finsh)
        installer.button_toggle_signal.connect(self.app_card.switch_button)
        installer.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)
        installer.install_finish_signal.connect(self.on_install_finsh)

        QThreadPool.globalInstance().start(installer)

    @Slot()
    def on_install_finsh(self) -> None:
        """处理安装完成逻辑"""
        success_bar(self.tr("安装成功 !"))
        self.app_card.switch_button(ButtonStatus.INSTALL)

    @Slot()
    def on_error_finsh(self) -> None:
        """处理错误结束逻辑"""
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.update_page()  # 刷新页面状态
