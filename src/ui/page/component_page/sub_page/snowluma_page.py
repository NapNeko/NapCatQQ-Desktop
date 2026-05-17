# -*- coding: utf-8 -*-
"""SnowLuma 适配 P6.3.1: 组件页 SnowLuma 安装/更新页面.

与 :class:`NapCatPage` 1:1 复刻同款骨架, 区别:

- ``app_card.set_name`` 显示 "SnowLuma" + 仓库链接 :data:`Urls.SNOWLUMA_REPO`
- 下载链接由 :meth:`Urls.get_snowluma_download_url` 动态构造 (含版本号)
- 安装器走 :class:`SnowLumaInstall` (而非 :class:`NapCatInstall`)
- **不做** SHA512 完整性校验 (上游 SnowLuma 未提供 release.json hash 服务;
  与 NapCat 不同, NapCat 走 [`run_napcat_archive_hash_check`](src/ui/components/install_hash_check.py))
- 本地版本由 :meth:`LocalVersionTask.get_snowluma_version` 提供, 读 ``.installed_tag``

Note (W6 2026-05-11): SnowLuma WebUI **全局密码 override** 卡片**不在本页**, 而是放在
设置页 → 常规 tab 的 ``SnowLuma`` 卡片组 (``setup_page/sub_page/general.py``); 该卡片
仅在 SnowLuma 已安装 (``PathFunc.get_snowluma_node_executable()`` 非空) 时可见.
本组件页只负责 SnowLuma 的安装 / 更新 / 卸载, 与 :class:`NapCatPage` 对齐.
"""
# 第三方库导入
from creart import it
from PySide6.QtCore import QThreadPool, QTimer, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.home import home_version_refresh_bus
from src.core.versioning import LocalVersionTask, VersionSnapshot
from src.core.installation.installers import SnowLumaInstall
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import summarize_path
from src.core.runtime.paths import PathFunc
from src.core.runtime.bot_process_manager import BotProcessManager
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from ..utils import ButtonStatus
from ..widget import PageBase


class SnowLumaPage(PageBase):
    """SnowLuma 核心库的安装, 更新和管理页面 (P1 SnowLuma 适配)."""

    def __init__(self, parent) -> None:
        """初始化 SnowLuma 页面

        Args:
            parent: 父级控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UnitSnowLumaPage")
        self.downloader = None
        self.installer = None
        self.app_card.set_name("SnowLuma")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.SNOWLUMA_REPO.value)
        self.log_card.set_loading(True)
        self.log_card.set_url(Urls.SNOWLUMA_REPO.value.url())

        # 连接信号槽 (与 NapCatPage 同名同语义)
        self.app_card.install_button.clicked.connect(self.handle_download_requested)
        self.app_card.update_button.clicked.connect(self.handle_download_requested)
        self.app_card.pause_button.clicked.connect(self.handle_pause_requested)
        self.app_card.cancel_button.clicked.connect(self.handle_cancel_requested)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).snowluma_path))
        )

    # ==================== 公共方法 ====================
    def refresh_page_view(self) -> None:
        """根据本地和远程版本信息刷新页面状态."""
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
            error_bar(self.tr("无法获取 SnowLuma 远程版本, 请检查网络"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.set_log_markdown(self.remote_log)

    # ==================== 槽函数 ====================
    @Slot()
    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        """应用远程版本信息和更新日志."""
        if version_data.snowluma_version is None or version_data.snowluma_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("获取 SnowLuma 更新日志失败")
        else:
            self.remote_version = version_data.snowluma_version
            self.remote_log = version_data.snowluma_update_log

        self.mark_remote_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def apply_local_version_data(self, version_data: VersionSnapshot) -> None:
        """应用本地版本信息."""
        if version_data.snowluma_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.snowluma_version

        self.mark_local_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def handle_download_requested(self) -> None:
        """处理下载按钮点击事件, 开始下载 SnowLuma."""
        if self.is_operation_in_progress():
            logger.warning("SnowLuma 下载请求已忽略: 当前已有任务正在执行", log_source=LogSource.UI)
            info_bar(self.tr("SnowLuma 正在下载或安装，请稍候"))
            self.restore_operation_view()
            return

        if self.remote_version is None:
            logger.warning("SnowLuma 下载请求已忽略: 远程版本未就绪", log_source=LogSource.UI)
            error_bar(self.tr("远程版本未就绪, 请先检查网络连接"))
            return

        logger.info(
            f"请求下载/更新 SnowLuma: local={self.local_version}, remote={self.remote_version}",
            log_source=LogSource.UI,
        )
        if it(BotProcessManager).has_running_bot():
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            box = AskBox(
                self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), it(MainWindow)
            )
            box.yesButton.setText(self.tr("关闭全部"))

            if box.exec():
                logger.warning("SnowLuma 安装前关闭全部 Bot 以继续执行", log_source=LogSource.UI)
                it(BotProcessManager).stop_all_bots()
            else:
                logger.info("SnowLuma 安装流程取消: 用户拒绝关闭运行中的 Bot", log_source=LogSource.UI)
                return

        self.begin_download_operation(self.tr("正在准备下载 SnowLuma..."))
        info_bar(self.tr("正在下载 SnowLuma"))

        self._start_download()

    def _start_download(self) -> None:
        """启动或继续 SnowLuma 下载."""
        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        download_url = Urls.get_snowluma_download_url(self.remote_version)
        downloader = GithubDownloader(download_url)
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
        """暂停或继续当前 SnowLuma 下载."""
        if self.is_operation_paused():
            logger.info("SnowLuma 下载继续", log_source=LogSource.UI)
            self.resume_operation(self.tr("正在继续下载 SnowLuma..."))
            self._start_download()
            return

        if self.downloader is None:
            return

        logger.info("SnowLuma 收到暂停下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在暂停 SnowLuma 下载..."))
        self.downloader.request_pause()

    @Slot()
    def handle_cancel_requested(self) -> None:
        """取消当前 SnowLuma 下载."""
        if self.remote_version is None:
            self.end_operation()
            self.downloader = None
            self.refresh_page_view()
            return

        download_url = Urls.get_snowluma_download_url(self.remote_version)
        package_path = it(PathFunc).tmp_path / download_url.fileName()

        if self.is_operation_paused():
            from src.core.network.downloader import DownloaderBase

            DownloaderBase.safe_unlink(package_path.with_name(f"{package_path.name}.part"))
            self.end_operation()
            self.downloader = None
            self.refresh_page_view()
            info_bar(self.tr("已取消 SnowLuma 下载"))
            return

        if self.downloader is None:
            return

        logger.info("SnowLuma 收到取消下载请求", log_source=LogSource.UI)
        self.update_operation_status_text(self.tr("正在取消 SnowLuma 下载..."))
        self.downloader.request_cancel()

    @Slot()
    def handle_install_requested(self) -> None:
        """下载完成后开始安装 SnowLuma.

        与 :class:`NapCatPage.handle_install_requested` 的关键区别:
        SnowLuma 上游未提供 release.json hash 服务, 这里**不**做 SHA512 校验;
        如果未来上游提供, 可参照 NapCat 加 ``run_napcat_archive_hash_check`` 同款流程.
        """
        logger.info("SnowLuma 下载完成，开始安装", log_source=LogSource.UI)
        self.downloader = None

        if self.remote_version is None:
            error_bar(self.tr("远程版本未就绪, 安装中止"))
            self.handle_operation_failed()
            return

        success_bar(self.tr("下载成功, 正在安装..."))
        self.begin_install_operation(self.tr("正在安装 SnowLuma"))
        installer = SnowLumaInstall(tag=self.remote_version)
        self.installer = installer
        installer.status_label_signal.connect(self.update_operation_status_text)
        installer.error_finish_signal.connect(self.handle_operation_failed)
        installer.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)
        installer.install_finish_signal.connect(self.handle_install_finished)

        QThreadPool.globalInstance().start(installer)

    @Slot()
    def handle_download_paused(self) -> None:
        """处理 SnowLuma 下载暂停."""
        self.downloader = None
        self.pause_operation(self.tr("SnowLuma 下载已暂停"))

    @Slot()
    def handle_download_canceled(self) -> None:
        """处理 SnowLuma 下载取消."""
        self.downloader = None
        self.end_operation()
        self.refresh_page_view()
        info_bar(self.tr("已取消 SnowLuma 下载"))

    @Slot()
    def handle_install_finished(self) -> None:
        """安装完成后的处理逻辑."""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.info(
            f"SnowLuma 安装完成: path={summarize_path(it(PathFunc).snowluma_path)}",
            log_source=LogSource.UI,
        )
        success_bar(self.tr("安装成功 !"))
        self.local_version = LocalVersionTask().get_snowluma_version()
        if self.local_version is None and self.remote_version is not None:
            # 安装线程刚结束时, .installed_tag 可能还没被文件系统同步可见;
            # 先按已安装的远程版本更新 UI, 再补一次完整刷新做最终校准.
            self.local_version = self.remote_version
        self.refresh_page_view()
        QTimer.singleShot(300, self._refresh_version_state_after_install)

    def _refresh_version_state_after_install(self) -> None:
        """安装完成后补一次完整版本刷新, 确保按钮状态与本地版本一致."""
        parent = self.parent()
        if parent is not None and hasattr(parent, "refresh_versions"):
            logger.info("SnowLuma 安装完成后触发一次版本校准刷新", log_source=LogSource.UI)
            parent.refresh_versions()
        home_version_refresh_bus.request_refresh()

    @Slot()
    def handle_operation_failed(self) -> None:
        """下载或安装过程中发生错误时的处理逻辑."""
        self.end_operation()
        self.downloader = None
        self.installer = None
        logger.error("SnowLuma 下载或安装流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.refresh_page_view()
