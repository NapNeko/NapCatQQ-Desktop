# -*- coding: utf-8 -*-
# 标准库导入
import winreg

# 第三方库导入
import httpx
from qfluentwidgets import (
    BodyLabel,
    ImageLabel,
    ProgressBar,
    SubtitleLabel,
    PrimaryPushButton,
    IndeterminateProgressBar,
)
from qframelesswindow import FramelessWindow
from PySide6.QtGui import QPixmap
from PySide6.QtCore import Qt, QUrl
from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.ui.Icon import StaticIcon as SI
from src.ui.Icon import NapCatDesktopIcon as DI
from src.ui.common.info_bar import success_bar
from src.ui.UnitPage.status import StatusLabel, ButtonStatus, ProgressRingStatus
from src.core.utils.path_func import PathFunc
from src.core.network.urls import Urls
from src.ui.common.message_box import FolderBox
from src.core.utils.install_func import QQInstall, NapCatInstall
from src.core.network.downloader import QQDownloader, GithubDownloader


class InstallPageBase(QWidget):

    # 按钮显示标识符
    button_visibility: dict = {
        ButtonStatus.INSTALL: {"install": False, "update": False, "openFolder": True},
        ButtonStatus.UNINSTALLED: {"install": True, "update": False, "openFolder": False},
        ButtonStatus.UPDATE: {"install": False, "update": True, "openFolder": False},
        ButtonStatus.NONE: {"install": False, "update": False, "openFolder": False},
    }

    # 进度条显示标识符
    progress_ring_visibility: dict = {
        ProgressRingStatus.INDETERMINATE: {"indeterminate": True, "determinate": False},
        ProgressRingStatus.DETERMINATE: {"indeterminate": False, "determinate": True},
        ProgressRingStatus.NONE: {"indeterminate": False, "determinate": False},
    }

    # 状态标签显示标识符
    status_label_visibility: dict = {StatusLabel.SHOW: True, StatusLabel.HIDE: False}

    def __init__(self, parent: FramelessWindow):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = SubtitleLabel(self)
        self.iconLabel = ImageLabel(self)
        self.installButton = PrimaryPushButton(self.tr("安装"), self)
        self.statusLabel = BodyLabel(self.tr("正在安装..."), self)
        self.indeterminateProgressRing = IndeterminateProgressBar(self)
        self.progressBar = ProgressBar(self)

        # 设置属性
        self.statusLabel.hide()
        self.indeterminateProgressRing.hide()
        self.progressBar.hide()

        self.progressBar.setTextVisible(True)
        self.progressBar.setFixedWidth(200)
        self.indeterminateProgressRing.setFixedWidth(200)
        self.installButton.setFixedWidth(175)

        # 布局
        self.vBoxLayout = QVBoxLayout(self)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(24)
        self.vBoxLayout.setAlignment(Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.addWidget(self.titleLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.iconLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.installButton, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.statusLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.indeterminateProgressRing, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.progressBar, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(1)

    def set_icon(self, icon: QPixmap | str):
        """设置图标"""
        if isinstance(icon, str):
            icon = QPixmap(icon)

        elif not isinstance(icon, QPixmap):
            raise TypeError("icon must be a QPixmap or str")

        self.iconLabel.setPixmap(icon)
        self.iconLabel.scaledToWidth(128)

    def set_title(self, title: str):
        """设置标题"""
        self.titleLabel.setText(title)

    def setStatusText(self, text: str) -> None:
        self.statusLabel.setText(text)

    def setProgressRingValue(self, value: int) -> None:
        self.progressBar.setValue(value)

    def setVisibility(self, visible_buttons: dict, visible_progress_rings: dict, visible_status_label: dict) -> None:
        """
        ## 设置按钮和进度环以及状态标签的可见性

        ## 参数
            - visible_buttons: dict, 按钮可见性
            - visible_progress_rings: dict, 进度环可见性
            - visible_status_label: dict, 状态标签可见性
        """
        self.installButton.setVisible(visible_buttons.get("install", False))

        self.indeterminateProgressRing.setVisible(visible_progress_rings.get("indeterminate", False))
        self.progressBar.setVisible(visible_progress_rings.get("determinate", False))

        self.statusLabel.setVisible(visible_status_label)

    def switchButton(self, status: ButtonStatus) -> None:
        """
        ## 切换按钮, 如果需要显示按钮, 则隐藏所有进度条
        """
        self.setVisibility(
            self.button_visibility[status],
            self.progress_ring_visibility[ProgressRingStatus.NONE],
            self.status_label_visibility[StatusLabel.HIDE],
        )

    def switchProgressRing(self, status: ProgressRingStatus) -> None:
        """
        ## 切换进度环, 如果需要显示进度条, 则隐藏所有按钮, 并显示 statusLabel
        """
        self.setVisibility(
            self.button_visibility[ButtonStatus.NONE],
            self.progress_ring_visibility[status],
            self.status_label_visibility[StatusLabel.SHOW],
        )


class InstallQQPage(InstallPageBase):
    """安装页面"""

    def __init__(self, parent: FramelessWindow) -> None:
        super().__init__(parent)
        # 创建杂七杂八的控件
        self.url = self.get_download_url()
        self.file_path = PathFunc().tmp_path / self.url.fileName()
        self.downloader = QQDownloader(self.url)

        # 设置属性
        self.set_icon(DI.QQ.path())
        self.set_title(self.tr("安装 QQ 客户端"))

        # 信号连接
        self.installButton.clicked.connect(self._onDownloadSlot)

    def get_download_url(self) -> str | None:
        """
        ## 获取下载链接
        """
        response = httpx.get(Urls.QQ_Version.value.url())
        ver_hash = response.json()["verHash"]
        version = response.json()["version"].replace("-", ".")
        download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
        return QUrl(download_url)

    def _onDownloadSlot(self) -> None:
        """
        ## 下载按钮槽函数
        """
        self.downloader.downloadProgress.connect(self.setProgressRingValue)
        self.downloader.downloadFinish.connect(self._onInstallSlot)
        self.downloader.statusLabel.connect(self.setStatusText)
        self.downloader.buttonToggle.connect(self.switchButton)
        self.downloader.progressRingToggle.connect(self.switchProgressRing)
        self.downloader.start()

    def _onInstallSlot(self) -> None:
        """
        ## 安装槽函数
        """
        # 项目内模块导入
        from src.ui.page.guide_window.guide_window import GuideWindow

        folder_box = FolderBox(self.tr("选择安装路径"), GuideWindow())
        folder_box.cancelButton.hide()
        folder_box.exec()

        # 修改注册表, 让安装程序读取注册表按照路径安装
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.getValue().replace("/", "\\"))
        winreg.CloseKey(key)

        # 开始安装
        self.installer = QQInstall(self.file_path)
        self.installer.statusLabel.connect(self.setStatusText)
        self.installer.buttonToggle.connect(self.switchButton)
        self.installer.progressRingToggle.connect(self.switchProgressRing)
        self.installer.installFinish.connect(self._installFinshSlot)

        self.installer.start()

    def _installFinshSlot(self) -> None:
        """
        ## 安装完成槽函数
        """
        # 项目内模块导入
        from src.ui.page.guide_window.guide_window import GuideWindow

        success_bar(self.tr("安装完成"), parent=GuideWindow())
        GuideWindow().on_next_page()


class InstallNapCatQQPage(InstallPageBase):

    def __init__(self, parent: FramelessWindow) -> None:
        super().__init__(parent)
        # 创建杂七杂八的控件
        self.url = Urls.NAPCATQQ_DOWNLOAD.value
        self.file_path = PathFunc().tmp_path / self.url.fileName()
        self.downloader = GithubDownloader(self.url)

        # 设置属性
        self.set_icon(SI.LOGO.path())
        self.set_title(self.tr("安装 NapCatQQ"))

        # 信号连接
        self.installButton.clicked.connect(self._onDownloadSlot)

    def _onDownloadSlot(self) -> None:
        """
        ## 下载按钮槽函数
        """
        self.downloader.downloadProgress.connect(self.setProgressRingValue)
        self.downloader.downloadFinish.connect(self._onInstallSlot)
        self.downloader.statusLabel.connect(self.setStatusText)
        self.downloader.buttonToggle.connect(self.switchButton)
        self.downloader.progressRingToggle.connect(self.switchProgressRing)
        self.downloader.start()

    def _onInstallSlot(self) -> None:
        """
        ## 安装槽函数
        """
        self.installer = NapCatInstall()
        self.installer.statusLabel.connect(self.setStatusText)
        self.installer.buttonToggle.connect(self.switchButton)
        self.installer.progressRingToggle.connect(self.switchProgressRing)
        self.installer.installFinish.connect(self._installFinshSlot)
        self.installer.start()

    def _installFinshSlot(self) -> None:
        """
        ## 安装完成槽函数
        """
        # 项目内模块导入
        from src.ui.page.guide_window.guide_window import GuideWindow

        success_bar(self.tr("安装完成"), parent=GuideWindow())
        GuideWindow().on_next_page()
