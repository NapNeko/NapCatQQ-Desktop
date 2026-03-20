# -*- coding: utf-8 -*-
# 第三方库导入
from creart import it
from PySide6.QtCore import QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.get_version import GetLocalVersionRunnable, VersionData
from src.core.utils.install_func import NapCatInstall
from src.core.utils.logger import LogSource, logger
from src.core.utils.logger.crash_bundle import summarize_path
from src.core.utils.path_func import PathFunc
from src.core.utils.run_napcat import ManagerNapCatQQProcess
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from src.ui.page.unit_page.base import PageBase
from src.ui.page.unit_page.status import ButtonStatus


class NapCatPage(PageBase):
    """NapCatQQ 核心库的安装、更新和管理页面"""

    def __init__(self, parent) -> None:
        """初始化 NapCat 页面

        Args:
            parent: 父级控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UnitNapCatPage")
        self.app_card.set_name("NapCatQQ")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.NAPCATQQ_REPO.value)
        self.log_card.set_loading(True)

        # 连接信号槽
        self.app_card.install_button.clicked.connect(self.on_download)
        self.app_card.update_button.clicked.connect(self.on_download)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).napcat_path))
        )

    # ==================== 公共方法 ====================
    def update_page(self) -> None:
        """根据本地和远程版本信息更新页面状态"""
        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            self.log_card.setLog(self.remote_log)
            return

        if self.remote_version is None:
            # 如果没有远程版本则提示错误
            error_bar(self.tr("无法获取 NapCatQQ 版本信息, 请检查网络连接"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.setLog(self.remote_log)

    # ==================== 槽函数 ====================
    @Slot()
    def on_update_remote_version(self, version_data: VersionData) -> None:
        """更新远程版本信息和更新日志"""
        if version_data.napcat_version is None or version_data.napcat_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("获取 NapCatQQ 更新日志失败")
        else:
            self.remote_version = version_data.napcat_version
            self.remote_log = version_data.napcat_update_log

        self.mark_remote_version_loaded()
        self.update_page_if_ready()

    @Slot()
    def on_update_local_version(self, version_data: VersionData) -> None:
        """更新本地版本信息"""
        if version_data.napcat_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.napcat_version

        self.mark_local_version_loaded()
        self.update_page_if_ready()

    @Slot()
    def on_download(self) -> None:
        """处理下载按钮点击事件，开始下载 NapCat"""
        logger.info(
            f"请求下载/更新 NapCat: local={self.local_version}, remote={self.remote_version}",
            log_source=LogSource.UI,
        )
        if it(ManagerNapCatQQProcess).has_running_bot():
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            box = AskBox(
                self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), it(MainWindow)
            )
            box.yesButton.setText(self.tr("关闭全部"))

            if box.exec():
                logger.warning("NapCat 安装前关闭全部 Bot 以继续执行", log_source=LogSource.UI)
                it(ManagerNapCatQQProcess).stop_all_processes()
            else:
                logger.info("NapCat 安装流程取消: 用户拒绝关闭运行中的 Bot", log_source=LogSource.UI)
                return

        info_bar(self.tr("正在下载 NapCat"))

        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        downloader = GithubDownloader(Urls.NAPCATQQ_DOWNLOAD.value)
        downloader.download_progress_signal.connect(self.app_card.set_progress_ring_value)
        downloader.download_finish_signal.connect(self.on_install)
        downloader.status_label_signal.connect(self.app_card.set_status_text)
        downloader.error_finsh_signal.connect(self.on_error_finsh)
        downloader.button_toggle_signal.connect(self.app_card.switch_button)
        downloader.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)

        QThreadPool.globalInstance().start(downloader)

    @Slot()
    def on_install(self) -> None:
        """下载完成后开始安装 NapCat"""
        logger.info("NapCat 下载完成，开始安装", log_source=LogSource.UI)
        success_bar(self.tr("下载成功, 正在安装..."))
        installer = NapCatInstall()
        installer.status_label_signal.connect(self.app_card.set_status_text)
        installer.error_finish_signal.connect(self.on_error_finsh)
        installer.button_toggle_signal.connect(self.app_card.switch_button)
        installer.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)
        installer.install_finish_signal.connect(self.on_install_finsh)

        QThreadPool.globalInstance().start(installer)

    @Slot()
    def on_install_finsh(self) -> None:
        """安装完成后的处理逻辑"""
        logger.info(f"NapCat 安装完成: path={summarize_path(it(PathFunc).napcat_path)}", log_source=LogSource.UI)
        success_bar(self.tr("安装成功 !"))
        self.local_version = GetLocalVersionRunnable().get_napcat_version()
        self.update_page()

    @Slot()
    def on_error_finsh(self) -> None:
        """下载或安装过程中发生错误时的处理逻辑"""
        logger.error("NapCat 下载或安装流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.update_page()  # 刷新一次页面
