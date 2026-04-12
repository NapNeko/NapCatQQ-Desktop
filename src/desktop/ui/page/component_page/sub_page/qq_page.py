# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path
from typing import TYPE_CHECKING, cast

from PySide6.QtCore import QThreadPool, QUrl, Slot
from creart import it
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.home import home_version_refresh_bus
from src.core.versioning import LocalVersionTask, VersionSnapshot
from src.core.installation.installers import QQInstall
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import summarize_path, summarize_url
from src.core.runtime.paths import PathFunc
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox, FolderBox
from ..utils import ButtonStatus
from ..widget import PageBase

if TYPE_CHECKING:
    # 项目内模块导入
    from src.core.network.downloader import QQDownloader

    QQDownloaderType = QQDownloader | None

DESCRIPTION_TEXT = """
## **NapCatQQ与NTQQ的奇幻物语**  

在科技与魔法交织的NapCatQQ Robot世界, 存在着名为**NTQQ**的混沌之力。这股足以扭曲现实的力量, 曾令无数试图驾驭它的勇者湮灭在历史长河中——直到那只被遗弃的橘色猫娘出现。  

蜷缩在废墟里的NapCatQQ发现JavaScript代码块时, 还以为找到了新玩具。  
**"喵? 硬硬的……不能吃呢……"**  
她无意识地用肉垫拍打键盘, 却意外打通了与NTQQ的量子纠缠。从此, 这只走路都会左脚绊右脚的猫娘, 获得了通过代码改变现实的能力——尽管她自己完全不懂原理。  

**"天空变成棉花糖了喵? 月亮被吃掉了喵? "**  
随着NapCatQQ的胡乱敲击, 世界开始呈现荒诞的叠加态。七彩暴雨淹没城市, 时停的蝴蝶堵塞交通……人们终于意识到, 最可怕的不是力量本身, 而是力量掌握在一只根本不懂自己在做什么的萌物手里。  

**转机：秩序降临**  
直到那位戴着圆框眼镜的架构师出现。他带来的不是宝剑, 而是一卷泛着蓝光的**OneBot规范文档**。  
**"所有NTQQ调用必须经过WS/HTTP协议封装。"**  
这条铁律为混沌设立了边界。现在, NapCatQQ每次想改变现实, 都得乖乖把意图写成标准化API请求。  

**"POST……喵? 200状态码……是成功的意思喵? "**  
猫娘歪着头戳弄终端, 她依然会把响应体当成小鱼干图案, 但至少世界不再因她的一个喷嚏而维度折叠。那些曾被彩虹云朵和会说话的松鼠搞怕了的居民们, 也逐渐学会欣赏这份笨拙的可爱——毕竟当毛茸茸的尾巴扫过键盘时, 连报错信息都会变成爱心形状。  

**为什么是QQ? **  
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
        self.downloader: "QQDownloaderType" = None
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
        self.log_card.set_log_markdown(DESCRIPTION_TEXT)

    def _connect_signals(self) -> None:
        """连接信号与槽"""
        self.app_card.install_button.clicked.connect(self.handle_download_requested)
        self.app_card.update_button.clicked.connect(self.handle_download_requested)
        self.app_card.pause_button.clicked.connect(self.handle_pause_requested)
        self.app_card.cancel_button.clicked.connect(self.handle_cancel_requested)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(qq_path))
            if (qq_path := it(PathFunc).get_qq_path()) is not None
            else error_bar(self.tr("未检测到 QQ 安装路径"))
        )

    def refresh_page_view(self) -> None:
        """根据本地和远程版本信息刷新页面状态。"""
        if self.restore_operation_view():
            return

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
    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        """应用远程版本信息。"""
        if version_data.qq_version is None or version_data.qq_download_url is None:
            self.remote_version = None
            self.url = None
        else:
            self.remote_version = version_data.qq_version
            self.url = QUrl(version_data.qq_download_url)

        self.mark_remote_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def apply_local_version_data(self, version_data: VersionSnapshot) -> None:
        """应用本地版本信息。"""
        if version_data.qq_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.qq_version

        self.mark_local_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def handle_download_requested(self) -> None:
        """处理下载按钮点击事件，开始下载 QQ 安装包。"""
        if self.is_operation_in_progress():
            logger.warning("QQ 下载请求已忽略: 当前已有任务正在执行", log_source=LogSource.UI)
            info_bar(self.tr("QQ 正在下载或安装，请稍候"))
            self.restore_operation_view()
            return

        if self.url is None:
            logger.error("QQ 下载流程失败: 下载链接为空", log_source=LogSource.UI)
            error_bar(self.tr("QQ下载链接为空"))
            return

        logger.info(
            (
                "请求下载/更新 QQ: "
                f"local={self.local_version}, remote={self.remote_version}, "
                f"source={summarize_url(self.url.toString())}"
            ),
            log_source=LogSource.UI,
        )

        # 项目内模块导入
        from src.core.network.downloader import QQDownloader

        self.begin_download_operation(self.tr("正在准备下载 QQ..."))
        self.file_path = it(PathFunc).tmp_path / self.url.fileName()
        self._start_download()

    def _start_download(self) -> None:
        """启动或继续 QQ 下载。"""
        # 项目内模块导入
        from src.core.network.downloader import QQDownloader

        self.downloader = QQDownloader(self.url)
        self.downloader.download_progress_signal.connect(self.update_operation_progress_value)
        self.downloader.download_finish_signal.connect(self.handle_install_requested)
        self.downloader.download_paused_signal.connect(self.handle_download_paused)
        self.downloader.download_canceled_signal.connect(self.handle_download_canceled)
        self.downloader.status_label_signal.connect(self.update_operation_status_text)
        self.downloader.error_finsh_signal.connect(self.handle_operation_failed)
        self.downloader.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)

        QThreadPool.globalInstance().start(self.downloader)

    @Slot()
    def handle_pause_requested(self) -> None:
        """暂停或继续当前 QQ 下载。"""
        if self.is_operation_paused():
            logger.info("QQ 下载继续", log_source=LogSource.UI)
            self.resume_operation(self.tr("正在继续下载 QQ..."))
            self._start_download()
            return

        if self.downloader is None:
            return

        logger.info("QQ 收到暂停下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在暂停 QQ 下载..."))
        self.downloader.request_pause()

    @Slot()
    def handle_cancel_requested(self) -> None:
        """取消当前 QQ 下载。"""
        if self.file_path is None:
            return

        if self.is_operation_paused():
            from src.core.network.downloader import DownloaderBase

            DownloaderBase.safe_unlink(self.file_path.with_name(f"{self.file_path.name}.part"))
            self.end_operation()
            self.downloader = None
            self.refresh_page_view()
            info_bar(self.tr("已取消 QQ 下载"))
            return

        if self.downloader is None:
            return

        logger.info("QQ 收到取消下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在取消 QQ 下载..."))
        self.downloader.request_cancel()

    @Slot()
    def handle_install_requested(self) -> None:
        """处理下载完成后的安装逻辑。"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        logger.info("QQ 下载完成，进入安装流程", log_source=LogSource.UI)
        success_bar(self.tr("下载成功, 正在安装..."))
        self.downloader = None
        self.begin_install_operation(self.tr("正在安装 QQ"))

        # 创建询问弹出框
        folder_box = FolderBox(self.tr("选择安装路径"), it(MainWindow))

        if not folder_box.exec():
            # 如果没有点击确定按钮
            if self.file_path is not None:
                self.file_path.unlink()
            self.end_operation()
            self.downloader = None
            self.installer = None
            logger.info("QQ 安装流程取消: 未确认安装路径", log_source=LogSource.UI)
            info_bar(self.tr("取消安装"))
            self.refresh_page_view()
            return

        # 修改注册表, 设置安装路径
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.get_value().replace("/", "\\"))
        winreg.CloseKey(key)

        # 检查是否存在 dbghelp.dll 文件, 否则会导致安装失败
        if Path(Path(folder_box.get_value()) / "dbghelp.dll").exists():
            rm_dll_box = AskBox(
                self.tr("检测到修补文件"), self.tr("您需要删除 dbghelp.dll 才能正确安装QQ"), it(MainWindow)
            )
            rm_dll_box.yesButton.setText(self.tr("删除"))
            if rm_dll_box.exec():
                # 用户点击了删除
                Path(Path(folder_box.get_value()) / "dbghelp.dll").unlink()
                logger.warning("QQ 安装前已移除冲突文件 dbghelp.dll", log_source=LogSource.UI)
            else:
                if self.file_path is not None:
                    self.file_path.unlink()
                self.end_operation()
                self.downloader = None
                self.installer = None
                logger.info("QQ 安装流程取消: 用户拒绝删除 dbghelp.dll", log_source=LogSource.UI)
                info_bar(self.tr("取消安装"))
                self.refresh_page_view()
                return

        # 开始安装
        if self.file_path is None:
            logger.error("QQ 安装流程失败: 安装包路径为空", log_source=LogSource.UI)
            error_bar(self.tr("安装包路径为空"))
            return

        self.installer = QQInstall(self.file_path)
        self.installer.status_label_signal.connect(self.update_operation_status_text)
        self.installer.error_finish_signal.connect(self.handle_operation_failed)
        self.installer.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)
        self.installer.install_finish_signal.connect(self.handle_install_finished)

        QThreadPool.globalInstance().start(self.installer)

    @Slot()
    def handle_download_paused(self) -> None:
        """处理 QQ 下载暂停。"""
        self.downloader = None
        self.pause_operation(self.tr("QQ 下载已暂停"))

    @Slot()
    def handle_download_canceled(self) -> None:
        """处理 QQ 下载取消。"""
        self.downloader = None
        self.end_operation()
        self.refresh_page_view()
        info_bar(self.tr("已取消 QQ 下载"))

    @Slot()
    def handle_install_finished(self) -> None:
        """处理安装完成逻辑。"""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.info(
            f"QQ 安装完成: installer={summarize_path(self.file_path) if self.file_path else '<empty-path>'}",
            log_source=LogSource.UI,
        )
        success_bar(self.tr("安装成功 !"))
        self.local_version = LocalVersionTask().get_qq_version()
        self.refresh_page_view()
        home_version_refresh_bus.request_refresh()

    @Slot()
    def handle_operation_failed(self) -> None:
        """处理错误结束逻辑。"""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.error("QQ 下载或安装流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.refresh_page_view()

