# -*- coding: utf-8 -*-
# 标准库导入
import subprocess
import sys

from PySide6.QtCore import QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.path_func import PathFunc
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox
from src.ui.page.bot_list_page import BotListWidget
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
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(PathFunc().napcat_path))
        )

    # ==================== 公共方法 ====================
    def update_page(self) -> None:
        """根据本地和远程版本信息更新页面状态"""
        if self.local_version is None:
            # 如果没有本地版本则显示安装按钮
            self.app_card.switch_button(ButtonStatus.UNINSTALLED)
            return

        if self.remoteVersion is None:
            # 如果没有远程版本则不操作
            return

        if self.remoteVersion != self.local_version:
            self.app_card.switch_button(ButtonStatus.UPDATE)
        else:
            self.app_card.switch_button(ButtonStatus.INSTALL)

        self.log_card.setLog(self.remoteLog)

    def on_update_remote_version(self) -> None:
        """更新远程版本信息和更新日志"""
        self.remoteVersion = self.get_version.remote_NCD
        self.remoteLog = self.get_version.updateLog_NCD
        self.update_page()

    def on_update_local_version(self) -> None:
        """更新本地版本信息"""
        self.local_version = self.get_version.local_NCD

    # ==================== 槽函数 ====================
    @Slot()
    def on_download(self) -> None:
        """处理下载按钮点击事件，开始下载应用程序"""
        if BotListWidget().get_bot_is_run():
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            box = AskBox(
                self.tr("失败"), self.tr("存在 Bot 运行,无法执行操作,是否关闭所有 Bot 以继续执行"), MainWindow()
            )
            box.yesButton.clicked.connect(BotListWidget().stop_all_bot)
            box.yesButton.setText(self.tr("关闭全部"))

            if not box.exec():
                return

        info_bar(self.tr("正在下载 NapCat Desktop"))

        # 项目内模块导入
        from src.core.network.downloader import GithubDownloader

        self.downloader = GithubDownloader(Urls.NCD_DOWNLOAD.value)
        self.downloader.download_progress_signal.connect(self.app_card.set_progress_ring_value)
        self.downloader.download_finish_signal.connect(self.on_install)
        self.downloader.status_label_signal.connect(self.app_card.set_status_text)
        self.downloader.error_finsh_signal.connect(self.on_error_finsh)
        self.downloader.button_toggle_signal.connect(self.app_card.switch_button)
        self.downloader.progress_ring_toggle_signal.connect(self.app_card.switch_progress_ring)
        self.downloader.start()

    @Slot()
    def on_install(self) -> None:
        """下载完成后执行安装逻辑"""
        success_bar(self.tr("下载成功, 正在安装..."))

        # 写入安装脚本
        bat_content = "\n".join(
            [
                "@echo off",
                "setlocal",
                "",
                "rem 定义应用程序路径",
                'set "app_name=NapCatQQ-Desktop.exe"',
                'set "current_app_path=%~dp0%app_name%"',
                'set "new_app_path=%~dp0tmp\\%app_name%"',
                "",
                "rem 等待旧版进程退出，如果其还在运行的话",
                ":wait_for_exit",
                'tasklist /FI "IMAGENAME eq %app_name%" 2>NUL | find /I /N "%app_name%">NUL',
                'if "%ERRORLEVEL%"=="0" (',
                "    echo Waiting for the application to exit...",
                "    timeout /T 1 /NOBREAK > NUL",
                "    goto wait_for_exit",
                ")",
                "",
                "rem 删除旧版本",
                'if exist "%current_app_path%" (',
                '    del "%current_app_path%"',
                ")",
                "",
                "rem 等待3秒钟",
                "timeout /T 3 /NOBREAK > NUL",
                "",
                "rem 移动新版到应用程序目录",
                'move "%new_app_path%" "%current_app_path%"',
                "",
                "rem 启动新版本应用程序",
                'start "" "%current_app_path%"',
                "",
                "rem 删除自身",
                'del "%~f0"',
                "endlocal",
            ]
        )
        with open(str(PathFunc().base_path / "update.bat"), "w", encoding="utf-8") as file:
            file.write(bat_content)

        # 创建进程
        subprocess.Popen([str(PathFunc().base_path / "update.bat")], shell=True)

        # 退出程序
        sys.exit()

    @Slot()
    def on_error_finsh(self) -> None:
        """下载错误处理逻辑"""
        error_bar(self.tr("下载时发生错误, 详情查看 设置 > Log"))
        self.update_page()  # 刷新一次页面
