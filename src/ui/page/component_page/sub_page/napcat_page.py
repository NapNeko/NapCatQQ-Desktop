# -*- coding: utf-8 -*-
# 第三方库导入
from creart import it
from PySide6.QtCore import QThreadPool, QTimer, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.home import home_version_refresh_bus
from src.core.versioning import LocalVersionTask, VersionSnapshot
from src.core.installation.installers import NapCatInstall
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import summarize_path
from src.core.runtime.paths import PathFunc
from src.core.runtime.napcat import ManagerNapCatQQProcess
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from ..utils import ButtonStatus
from ..widget import PageBase


class NapCatPage(PageBase):
    """NapCatQQ 核心库的安装、更新和管理页面"""

    def __init__(self, parent) -> None:
        """初始化 NapCat 页面

        Args:
            parent: 父级控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UnitNapCatPage")
        self.downloader = None
        self.installer = None
        self.app_card.set_name("NapCatQQ")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.NAPCATQQ_REPO.value)
        self.log_card.set_loading(True)

        # 连接信号槽
        self.app_card.install_button.clicked.connect(self.handle_download_requested)
        self.app_card.update_button.clicked.connect(self.handle_download_requested)
        self.app_card.pause_button.clicked.connect(self.handle_pause_requested)
        self.app_card.cancel_button.clicked.connect(self.handle_cancel_requested)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).napcat_path))
        )

    # ==================== 公共方法 ====================
    def refresh_page_view(self) -> None:
        """根据本地和远程版本信息刷新页面状态。"""
        if self.restore_operation_view():
            self.log_card.set_log_markdown(self.remote_log)
            return

        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            self.log_card.set_log_markdown(self.remote_log)
            return

        if self.remote_version is None:
            # 如果没有远程版本则提示错误
            error_bar(self.tr("无法获取 NapCatQQ 版本信息, 请检查网络连接"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.set_log_markdown(self.remote_log)

    # ==================== 槽函数 ====================
    @Slot()
    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        """应用远程版本信息和更新日志。"""
        if version_data.napcat_version is None or version_data.napcat_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("获取 NapCatQQ 更新日志失败")
        else:
            self.remote_version = version_data.napcat_version
            self.remote_log = version_data.napcat_update_log

        self.mark_remote_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def apply_local_version_data(self, version_data: VersionSnapshot) -> None:
        """应用本地版本信息。"""
        if version_data.napcat_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.napcat_version

        self.mark_local_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def handle_download_requested(self) -> None:
        """处理下载按钮点击事件，开始下载 NapCat。"""
        if self.is_operation_in_progress():
            logger.warning("NapCat 下载请求已忽略: 当前已有任务正在执行", log_source=LogSource.UI)
            info_bar(self.tr("NapCat 正在下载或安装，请稍候"))
            self.restore_operation_view()
            return

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

        self.begin_download_operation(self.tr("正在准备下载 NapCat..."))
        info_bar(self.tr("正在下载 NapCat"))

        self._start_download()

    def _start_download(self) -> None:
        """启动或继续 NapCat 下载。"""

        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        downloader = GithubDownloader(Urls.NAPCATQQ_DOWNLOAD.value)
        self.downloader = downloader
        downloader.download_progress_signal.connect(self.update_operation_progress_value)
        downloader.download_finish_signal.connect(self.handle_install_requested)
        downloader.download_paused_signal.connect(self.handle_download_paused)
        downloader.download_canceled_signal.connect(self.handle_download_canceled)
        downloader.status_label_signal.connect(self.update_operation_status_text)
        downloader.error_finsh_signal.connect(self.handle_operation_failed)
        downloader.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)

        QThreadPool.globalInstance().start(downloader)

    @Slot()
    def handle_pause_requested(self) -> None:
        """暂停或继续当前 NapCat 下载。"""
        if self.is_operation_paused():
            logger.info("NapCat 下载继续", log_source=LogSource.UI)
            self.resume_operation(self.tr("正在继续下载 NapCat..."))
            self._start_download()
            return

        if self.downloader is None:
            return

        logger.info("NapCat 收到暂停下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在暂停 NapCat 下载..."))
        self.downloader.request_pause()

    @Slot()
    def handle_cancel_requested(self) -> None:
        """取消当前 NapCat 下载。"""
        package_path = it(PathFunc).tmp_path / Urls.NAPCATQQ_DOWNLOAD.value.fileName()

        if self.is_operation_paused():
            from src.core.network.downloader import DownloaderBase

            DownloaderBase.safe_unlink(package_path.with_name(f"{package_path.name}.part"))
            self.end_operation()
            self.downloader = None
            self.refresh_page_view()
            info_bar(self.tr("已取消 NapCat 下载"))
            return

        if self.downloader is None:
            return

        logger.info("NapCat 收到取消下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在取消 NapCat 下载..."))
        self.downloader.request_cancel()

    @Slot()
    def handle_install_requested(self) -> None:
        """下载完成后开始安装 NapCat。"""
        logger.info("NapCat 下载完成，开始安装", log_source=LogSource.UI)
        success_bar(self.tr("下载成功, 正在安装..."))
        self.downloader = None
        self.begin_install_operation(self.tr("正在安装 NapCat"))
        installer = NapCatInstall()
        self.installer = installer
        installer.status_label_signal.connect(self.update_operation_status_text)
        installer.error_finish_signal.connect(self.handle_operation_failed)
        installer.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)
        installer.install_finish_signal.connect(self.handle_install_finished)

        QThreadPool.globalInstance().start(installer)

    @Slot()
    def handle_download_paused(self) -> None:
        """处理 NapCat 下载暂停。"""
        self.downloader = None
        self.pause_operation(self.tr("NapCat 下载已暂停"))

    @Slot()
    def handle_download_canceled(self) -> None:
        """处理 NapCat 下载取消。"""
        self.downloader = None
        self.end_operation()
        self.refresh_page_view()
        info_bar(self.tr("已取消 NapCat 下载"))

    @Slot()
    def handle_install_finished(self) -> None:
        """安装完成后的处理逻辑。"""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.info(f"NapCat 安装完成: path={summarize_path(it(PathFunc).napcat_path)}", log_source=LogSource.UI)
        success_bar(self.tr("安装成功 !"))
        self.local_version = LocalVersionTask().get_napcat_version()
        if self.local_version is None and self.remote_version is not None:
            # 安装线程刚结束时，package.json 在个别机器上可能仍是旧视图；
            # 先按已安装的远程版本更新 UI，再补一次完整刷新做最终校准。
            self.local_version = self.remote_version
        self.refresh_page_view()
        QTimer.singleShot(300, self._refresh_version_state_after_install)

    def _refresh_version_state_after_install(self) -> None:
        """安装完成后补一次完整版本刷新，确保按钮状态与本地版本一致。"""
        parent = self.parent()
        if parent is not None and hasattr(parent, "refresh_versions"):
            logger.info("NapCat 安装完成后触发一次版本校准刷新", log_source=LogSource.UI)
            parent.refresh_versions()
        home_version_refresh_bus.request_refresh()

    @Slot()
    def handle_operation_failed(self) -> None:
        """下载或安装过程中发生错误时的处理逻辑。"""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.error("NapCat 下载或安装流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.refresh_page_view()

