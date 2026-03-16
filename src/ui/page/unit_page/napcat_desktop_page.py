# -*- coding: utf-8 -*-
# 标准库导入
import os
import subprocess
import sys
from pathlib import Path

# 第三方库导入
from creart import it
from PySide6.QtCore import QThreadPool, QUrl, Slot
from PySide6.QtGui import QDesktopServices

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.desktop_update import UPDATE_ARCHIVE_NAME, prepare_desktop_update
from src.core.utils.get_version import VersionData
from src.core.utils.path_func import PathFunc
from src.core.utils.run_napcat import ManagerNapCatQQProcess
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
        if it(ManagerNapCatQQProcess).has_running_bot():

            from src.ui.window.main_window import MainWindow

            box = AskBox(self.tr("警告"), self.tr("目前还有 Bot 正在运行, 此操作会关闭所有 Bot"), it(MainWindow))
            box.yesButton.setText(self.tr("关闭全部"))

            if box.exec():
                it(ManagerNapCatQQProcess).stop_all_processes()
            else:
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
        success_bar(self.tr("下载成功, 正在准备目录版更新..."))

        base_path = it(PathFunc).base_path
        zip_path = it(PathFunc).tmp_path / UPDATE_ARCHIVE_NAME

        try:
            prepare_desktop_update(zip_path, base_path)
        except ValueError as exc:
            error_bar(str(exc))
            self.update_page()
            return

        # 从 Qt 资源读取安装脚本模板（优先），若失败则回退为一个最小脚本
        bat_content = self._load_update_script()
        bat_path = it(PathFunc).tmp_path / "update.bat"
        with open(str(bat_path), "w", encoding="utf-8") as file:
            file.write(bat_content)

        # 启动安装脚本（以系统方式打开，确保在主程序退出后依然运行）
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

    def _load_update_script(self) -> str:
        """读取更新脚本模板，失败时使用内置兜底脚本。"""

        try:
            script_path = Path(__file__).resolve().parents[3] / "resource" / "script" / "update.bat"
            return script_path.read_text(encoding="utf-8")
        except OSError:
            return self._fallback_update_script()

    def _fallback_update_script(self) -> str:
        """目录版更新脚本的最小兜底实现。"""

        return (
            "@echo off\n"
            "setlocal enabledelayedexpansion\n"
            'for %%I in ("%~dp0") do set "script_dir=%%~fI"\n'
            'for %%I in ("%script_dir%\\..\\..") do set "app_root=%%~fI"\n'
            'set "log=%app_root%\\update.log"\n'
            'set "staged_app_dir=%app_root%\\_update_staging\\package\\NapCatQQ-Desktop"\n'
            'set "staged_exe=%staged_app_dir%\\NapCatQQ-Desktop.exe"\n'
            'set "installed_exe=%app_root%\\NapCatQQ-Desktop.exe"\n'
            'if not exist "%staged_exe%" (echo staged package missing >> "%log%" & exit /b 1)\n'
            "set /a max_wait=60\n"
            "set /a waited=0\n"
            ":wait_for_exit\n"
            'tasklist /FI "IMAGENAME eq NapCatQQ-Desktop.exe" /NH | find /I "NapCatQQ-Desktop.exe" >NUL\n'
            'if "%ERRORLEVEL%"=="0" (\n'
            "    if !waited! GEQ !max_wait! (\n"
            '        taskkill /IM "NapCatQQ-Desktop.exe" /F >> "%log%" 2>&1\n'
            "    ) else (\n"
            "        set /a waited+=1\n"
            "        timeout /T 1 /NOBREAK > NUL\n"
            "        goto wait_for_exit\n"
            "    )\n"
            ")\n"
            'robocopy "%staged_app_dir%" "%app_root%" /MIR /R:3 /W:1 /NFL /NDL /NP /XD "%app_root%\\runtime" "%app_root%\\log" "%app_root%\\_update_staging" >> "%log%" 2>&1\n'
            'set "copy_rc=%ERRORLEVEL%"\n'
            "if !copy_rc! GEQ 8 exit /b 1\n"
            'if not exist "%installed_exe%" exit /b 1\n'
            'del /Q "%app_root%\\runtime\\tmp\\NapCatQQ-Desktop.zip" >> "%log%" 2>&1\n'
            'rmdir /S /Q "%app_root%\\_update_staging" >> "%log%" 2>&1\n'
            'start "" "%installed_exe%"\n'
            'del "%~f0"\n'
        )
