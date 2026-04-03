# -*- coding: utf-8 -*-
# 标准库导入
import os
import sys
from time import monotonic, sleep

# 第三方库导入
from creart import it
from PySide6.QtCore import QCoreApplication, QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.desktop_update import UpdateManager
from src.core.network.urls import Urls
from src.core.versioning import VersionSnapshot
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import summarize_path
from src.core.runtime.paths import PathFunc
from src.core.runtime.napcat import ManagerNapCatQQProcess
from src.resource import resource as _resource  # noqa: F401
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from ..utils import (
    ButtonStatus,
    build_update_confirmation_message,
    get_download_message,
    get_install_type_log_prefix,
)
from ..widget import PageBase


class DesktopPage(PageBase):
    """NapCatQQ Desktop 应用程序更新页面"""

    def __init__(self, parent) -> None:
        """初始化更新页面

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UpdateDesktopPage")
        self.app_card.set_name("NapCatQQ Desktop")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.NCD_REPO.value)
        self.log_card.set_loading(True)
        self._update_manager = UpdateManager()

        # 连接信号槽
        self.app_card.install_button.clicked.connect(self.handle_download_requested)
        self.app_card.update_button.clicked.connect(self.handle_download_requested)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(it(PathFunc).base_path)))
        )

    # ==================== 公共方法 ====================
    def refresh_page_view(self) -> None:
        """根据本地和远程版本信息刷新页面状态。"""
        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            self.log_card.set_log_markdown(self.remote_log)
            return

        if self.remote_version is None:
            # 如果没有远程版本则提示错误
            error_bar(self.tr("无法获取 NapCatQQ Desktop 版本信息, 请检查网络连接"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.set_log_markdown(self._get_display_log())

    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        """应用远程版本信息和更新日志。"""
        if version_data.ncd_version is None or version_data.ncd_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("无法获取 NapCat Desktop 版本信息, 请检查网络连接")
        else:
            self.remote_version = version_data.ncd_version
            self.remote_log = version_data.ncd_update_log

        self.mark_remote_version_loaded()
        self.refresh_page_if_ready()

    def apply_local_version_data(self, version_data: VersionSnapshot) -> None:
        """应用本地版本信息。"""
        if version_data.ncd_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.ncd_version

        self.mark_local_version_loaded()
        self.refresh_page_if_ready()

    # ==================== 槽函数 ====================
    @Slot()
    def handle_download_requested(self) -> None:
        """处理下载按钮点击事件，开始下载应用程序。"""
        logger.info(
            f"请求下载/更新 Desktop: local={self.local_version}, remote={self.remote_version}, "
            f"install_type={self._update_manager.install_type.value}",
            log_source=LogSource.UI,
        )

        from src.ui.window.main_window import MainWindow

        # 检查是否有运行的 Bot
        has_bot = it(ManagerNapCatQQProcess).has_running_bot()
        if has_bot:
            box = AskBox(
                self.tr("警告"),
                self.tr("目前还有 Bot 正在运行，此操作会关闭所有 Bot。\n\n更新前请确保所有 Bot 数据已保存。"),
                it(MainWindow),
            )
            box.yesButton.setText(self.tr("关闭全部并继续"))

            if box.exec():
                logger.warning("Desktop 更新前关闭全部 Bot 以继续执行", log_source=LogSource.UI)
                process_manager = it(ManagerNapCatQQProcess)
                process_manager.stop_all_processes()
                deadline = monotonic() + 5
                while process_manager.has_running_bot() and monotonic() < deadline:
                    sleep(0.1)

                if process_manager.has_running_bot():
                    logger.error("Desktop 更新前关闭 Bot 失败，仍有进程未退出", log_source=LogSource.UI)
                    error_bar(self.tr("仍有 Bot 未完全退出，请稍后重试更新。"))
                    return
            else:
                logger.info("Desktop 更新流程取消: 用户拒绝关闭运行中的 Bot", log_source=LogSource.UI)
                return

        # 显示更新确认弹窗（根据安装类型显示不同提示）
        if not self._show_update_confirmation(has_bot):
            logger.info("Desktop 更新流程取消: 用户在确认弹窗中选择取消", log_source=LogSource.UI)
            return

        info_bar(get_download_message(self.tr, self._update_manager.install_type))

        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        # 获取下载 URL
        download_url = self._get_download_url()
        downloader = GithubDownloader(download_url)
        downloader.download_progress_signal.connect(self.app_card.set_progress_ring_value)
        downloader.download_finish_signal.connect(self.handle_install_requested)
        downloader.status_label_signal.connect(self.app_card.set_status_text)
        downloader.error_finsh_signal.connect(self.handle_operation_failed)
        downloader.button_toggle_signal.connect(self.app_card.switch_button)
        downloader.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)

        QThreadPool.globalInstance().start(downloader)

    def _show_update_confirmation(self, has_running_bot: bool) -> bool:
        """显示更新确认弹窗，告知用户更新流程和注意事项。

        Args:
            has_running_bot: 是否有正在运行的 Bot

        Returns:
            bool: 用户是否确认继续
        """
        from src.ui.window.main_window import MainWindow

        full_message = build_update_confirmation_message(
            translate=self.tr,
            install_type=self._update_manager.install_type,
            local_version=self.local_version,
            remote_version=self.remote_version,
            has_running_bot=has_running_bot,
        )

        box = AskBox(self.tr("确认更新"), full_message, it(MainWindow))
        box.yesButton.setText(self.tr("开始更新"))
        box.cancelButton.setText(self.tr("取消"))

        from PySide6.QtWidgets import QDialog

        return box.exec() == QDialog.DialogCode.Accepted

    def _get_download_url(self) -> QUrl:
        """根据安装类型和版本获取下载 URL。

        Returns:
            QUrl: 下载链接
        """
        # 移除版本号中的 'v' 前缀
        version = self.remote_version
        if version and version.startswith("v"):
            version = version[1:]

        if not version:
            # 降级到默认链接
            return Urls.NCD_DOWNLOAD.value

        return Urls.get_ncd_download_url(version, "msi")

    @Slot()
    def handle_install_requested(self) -> None:
        """下载完成后执行安装逻辑。"""
        logger.info(
            f"Desktop 下载完成，开始准备更新: install_type={self._update_manager.install_type.value}",
            log_source=LogSource.UI,
        )
        success_bar(self.tr("下载成功, 正在准备更新..."))

        # 根据安装类型获取下载的文件路径
        package_filename = self._update_manager.get_update_filename(
            self.remote_version.lstrip("v") if self.remote_version else ""
        )
        package_path = it(PathFunc).tmp_path / package_filename

        try:
            # 使用 UpdateManager 准备和执行更新
            staging_path = self._update_manager.prepare_update(package_path)
            logger.info(f"更新包已准备: {summarize_path(staging_path)}", log_source=LogSource.UI)

            if self._update_manager.requires_msi_migration():
                logger.warning(
                    f"当前安装类型将迁移到 MSI 安装版: type={self._update_manager.install_type.value}",
                    log_source=LogSource.UI,
                )

            launch_result = self._update_manager.execute_update(staging_path, target_pid=os.getpid())
            if launch_result is None:
                raise RuntimeError("启动 MSI 更新失败")
            pid_text = launch_result.pid if launch_result.pid is not None else "unknown"
            logger.info(
                f"MSI 更新进程已启动: PID={pid_text}, package={summarize_path(staging_path)}",
                log_source=LogSource.UI,
            )

        except ValueError as exc:
            logger.error(f"准备更新失败: {exc}")
            error_bar(str(exc))
            self.refresh_page_view()
            return
        except OSError as exc:
            logger.error(f"更新流程发生文件系统错误: {exc}")
            error_bar(self.tr(f"更新失败: {exc}"))
            self.refresh_page_view()
            return
        except Exception as exc:
            logger.error(f"启动更新流程失败: {exc}")
            error_bar(self.tr(f"启动更新失败: {exc}"))
            self.refresh_page_view()
            return

        # 退出程序，安装脚本/MSI 会等待并替换可执行文件
        logger.warning("Desktop 更新已启动，当前进程准备退出", log_source=LogSource.UI)
        if not QThreadPool.globalInstance().waitForDone(5000):
            logger.warning("等待后台线程退出超时，继续执行应用退出", log_source=LogSource.UI)
        if (app := QCoreApplication.instance()) is not None:
            app.quit()
            return
        sys.exit(0)

    @Slot()
    def handle_operation_failed(self) -> None:
        """下载错误处理逻辑。"""
        logger.error("Desktop 下载或更新流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.refresh_page_view()

    def _get_display_log(self) -> str:
        """为更新日志追加安装类型提示。"""

        install_type_hint = get_install_type_log_prefix(self._update_manager.install_type)
        return install_type_hint + self.remote_log

