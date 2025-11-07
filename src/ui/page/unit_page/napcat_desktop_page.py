# -*- coding: utf-8 -*-
# 标准库导入
import subprocess
import sys
import os

# 第三方库导入
from creart import it
from PySide6.QtCore import QIODevice, QTextStream, QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.get_version import VersionData
from src.core.utils.path_func import PathFunc
from src.core.utils.run_napcat import ManagerNapCatQQProcess
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from src.ui.page.unit_page.base import PageBase
from src.ui.page.unit_page.status import ButtonStatus
from src.core.utils.file import QFluentFile


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
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).napcat_path))
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
        if it(ManagerNapCatQQProcess).has_running_bot():

            from src.ui.window.main_window import MainWindow

            box = AskBox(self.tr("警告"), self.tr("目前还有 Bot 正在运行, 此操作会关闭所有 Bot"), it(MainWindow))
            box.yesButton.setText(self.tr("关闭全部"))

            if box.exec():
                it(ManagerNapCatQQProcess).stop_all_processes()
            else:
                return

        info_bar(self.tr("正在下载 NapCat Desktop"))

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
        success_bar(self.tr("下载成功, 正在安装..."))

        # 从 Qt 资源读取安装脚本模板（优先），若失败则回退为一个最小脚本
        bat_content = ""
        try:
            with QFluentFile(":/script/update.bat", QIODevice.ReadOnly | QIODevice.Text) as qfile:
                ts = QTextStream(qfile)
                bat_content = ts.readAll()
        except Exception:
            bat_content = ""

        if not bat_content:
            # 回退：生成一个最小的脚本以确保更新能继续进行（非理想，但能作为兜底）
            bat_content = (
                "@echo off\n"
                "setlocal enabledelayedexpansion\n"
                'set "new_app_dir=%~dp0runtime\\tmp"\n'
                'set "new_app_path="\n'
                'set "new_file_name="\n'
                'for %%F in ("%new_app_dir%\\NapCatQQ-Desktop*.exe") do (\n'
                '    set "new_app_path=%%~fF"\n'
                '    set "new_file_name=%%~nxF"\n'
                "    goto :found_new\n"
                ")\n"
                ":found_new\n"
                "if not defined new_app_path (echo no new exe found && exit /b 1)\n"
                'set "current_app_path=%~dp0%new_file_name%"\n'
                'move /Y "%new_app_path%" "%current_app_path%"\n'
                'start "" "%current_app_path%"\n'
                'del "%~f0"\n'
            )
        with open(str(it(PathFunc).base_path / "update.bat"), "w", encoding="utf-8") as file:
            file.write(bat_content)

        # 启动安装脚本（以系统方式打开，确保在主程序退出后依然运行）
        bat_path = it(PathFunc).base_path / "update.bat"
        try:
            # 在 Windows 上优先使用 os.startfile，这会以默认程序方式运行 .bat 并立即返回
            os.startfile(str(bat_path))
        except Exception:
            # 回退：通过 cmd /c start 启动并尝试隐藏控制台窗口（静默启动）
            try:
                subprocess.Popen(["cmd", "/c", "start", '""', str(bat_path)], creationflags=subprocess.CREATE_NO_WINDOW)
            except Exception:
                # 最后回退为简单的 start 调用（保证不阻塞）
                subprocess.Popen(f'start "" "{str(bat_path)}"', shell=True)

        # 退出程序，安装脚本会等待并替换可执行文件
        sys.exit(0)

    @Slot()
    def on_error_finsh(self) -> None:
        """下载错误处理逻辑"""
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.update_page()  # 刷新一次页面
