# -*- coding: utf-8 -*-
# 标准库导入
import os
import subprocess
import sys
from pathlib import Path

# 第三方库导入
from creart import it
from PySide6.QtCore import QFile, QIODevice, QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.desktop_update import UPDATE_ARCHIVE_NAME, prepare_desktop_update
from src.core.utils.get_version import VersionData
from src.core.utils.logger import LogSource, logger
from src.core.utils.logger.crash_bundle import summarize_path
from src.core.utils.path_func import PathFunc
from src.core.utils.run_napcat import ManagerNapCatQQProcess
from src.resource import resource as _resource  # noqa: F401
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from src.ui.page.unit_page.base import PageBase
from src.ui.page.unit_page.status import ButtonStatus


class NCDPage(PageBase):
    """NapCatQQ Desktop 应用程序更新页面"""

    def __init__(self, parent) -> None:
        """初始化更新页面

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)
        self.setObjectName("UnitNCDPage")
        self.app_card.set_name("NapCatQQ Desktop")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.NCD_REPO.value)

        # 连接信号槽
        self.app_card.install_button.clicked.connect(self.on_download)
        self.app_card.update_button.clicked.connect(self.on_download)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(it(PathFunc).base_path)))
        )

    # ==================== 公共方法 ====================
    def update_page(self) -> None:
        """根据本地和远程版本信息更新页面状态"""
        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            return

        if self.remote_version is None:
            # 如果没有远程版本则提示错误
            error_bar(self.tr("无法获取 NapCatQQ Desktop 版本信息, 请检查网络连接"))
            return

        if self.remote_version != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.setLog(self.remote_log)

    def on_update_remote_version(self, version_data: VersionData) -> None:
        """更新远程版本信息和更新日志"""
        if version_data.ncd_version is None or version_data.ncd_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("无法获取 NapCat Desktop 版本信息, 请检查网络连接")
        else:
            self.remote_version = version_data.ncd_version
            self.remote_log = version_data.ncd_update_log

        self.update_page()

    def on_update_local_version(self, version_data: VersionData) -> None:
        """更新本地版本信息"""
        if version_data.ncd_version is None:
            self.local_version = None
        else:
            self.local_version = version_data.ncd_version

    # ==================== 槽函数 ====================
    @Slot()
    def on_download(self) -> None:
        """处理下载按钮点击事件，开始下载应用程序"""
        logger.info(
            f"请求下载/更新 Desktop: local={self.local_version}, remote={self.remote_version}",
            log_source=LogSource.UI,
        )
        if it(ManagerNapCatQQProcess).has_running_bot():

            from src.ui.window.main_window import MainWindow

            box = AskBox(self.tr("警告"), self.tr("目前还有 Bot 正在运行, 此操作会关闭所有 Bot"), it(MainWindow))
            box.yesButton.setText(self.tr("关闭全部"))

            if box.exec():
                logger.warning("Desktop 更新前关闭全部 Bot 以继续执行", log_source=LogSource.UI)
                it(ManagerNapCatQQProcess).stop_all_processes()
            else:
                logger.info("Desktop 更新流程取消: 用户拒绝关闭运行中的 Bot", log_source=LogSource.UI)
                return

        info_bar(self.tr("正在下载 NapCat Desktop 整包 ZIP"))

        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        downloader = GithubDownloader(Urls.NCD_DOWNLOAD.value)
        downloader.download_progress_signal.connect(self.app_card.set_progress_ring_value)
        downloader.download_finish_signal.connect(self.on_install)
        downloader.status_label_signal.connect(self.app_card.set_status_text)
        downloader.error_finsh_signal.connect(self.on_error_finsh)
        downloader.button_toggle_signal.connect(self.app_card.switch_button)
        downloader.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)

        QThreadPool.globalInstance().start(downloader)

    @Slot()
    def on_install(self) -> None:
        """下载完成后执行安装逻辑"""
        logger.info("Desktop 下载完成，开始准备目录版更新", log_source=LogSource.UI)
        success_bar(self.tr("下载成功, 正在准备目录版更新..."))

        base_path = it(PathFunc).base_path
        zip_path = it(PathFunc).tmp_path / UPDATE_ARCHIVE_NAME

        try:
            prepare_desktop_update(zip_path, base_path)
            bat_content = self._prepare_update_script()
            bat_path = it(PathFunc).tmp_path / "update.bat"
            bat_path.write_text(bat_content, encoding="utf-8")
            logger.info(
                (
                    "Desktop 更新脚本已生成: "
                    f"archive={summarize_path(zip_path)}, script={summarize_path(bat_path)}"
                ),
                log_source=LogSource.UI,
            )
            self._launch_update_script(bat_path)
        except ValueError as exc:
            logger.error(f"准备更新失败: {exc}")
            error_bar(str(exc))
            self.update_page()
            return
        except OSError as exc:
            logger.error(f"更新流程发生文件系统错误: {exc}")
            error_bar(self.tr(f"更新失败: {exc}"))
            self.update_page()
            return
        except Exception as exc:
            logger.error(f"启动更新流程失败: {exc}")
            error_bar(self.tr(f"启动更新失败: {exc}"))
            self.update_page()
            return

        # 退出程序，安装脚本会等待并替换可执行文件
        logger.warning("Desktop 更新脚本已启动，当前进程准备退出", log_source=LogSource.UI)
        sys.exit(0)

    @Slot()
    def on_error_finsh(self) -> None:
        """下载错误处理逻辑"""
        logger.error("Desktop 下载或更新流程失败", log_source=LogSource.UI)
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.update_page()  # 刷新一次页面

    @staticmethod
    def _load_update_script() -> str:
        """读取更新脚本模板，优先使用 Qt 资源，其次读取源码文件。"""

        resource_file = QFile(":/script/script/update.bat")
        if resource_file.open(QIODevice.OpenModeFlag.ReadOnly | QIODevice.OpenModeFlag.Text):
            try:
                return bytes(resource_file.readAll()).decode("utf-8")
            finally:
                resource_file.close()

        script_path = Path(__file__).resolve().parents[3] / "resource" / "script" / "update.bat"
        return script_path.read_text(encoding="utf-8")

    def _prepare_update_script(self) -> str:
        """生成带当前进程 PID 的更新脚本内容。"""

        script_content = self._load_update_script()
        target_pid_line = f'set "target_pid={os.getpid()}"'
        script_lines = script_content.splitlines()

        for index, line in enumerate(script_lines):
            if line.strip().lower() == "setlocal enabledelayedexpansion":
                script_lines.insert(index + 1, target_pid_line)
                break
        else:
            script_lines.insert(0, target_pid_line)

        return "\n".join(script_lines) + "\n"

    def _launch_update_script(self, bat_path: Path) -> None:
        """启动更新脚本，确保主程序退出后仍可继续执行。"""

        try:
            os.startfile(str(bat_path))
            return
        except OSError as exc:
            logger.warning(f"os.startfile 启动更新脚本失败，尝试回退方案: {exc}")

        try:
            subprocess.Popen(["cmd", "/c", "start", '""', str(bat_path)], creationflags=subprocess.CREATE_NO_WINDOW)
            return
        except OSError as exc:
            logger.warning(f"cmd start 启动更新脚本失败，尝试 shell 回退: {exc}")

        subprocess.Popen(f'start "" "{str(bat_path)}"', shell=True)
