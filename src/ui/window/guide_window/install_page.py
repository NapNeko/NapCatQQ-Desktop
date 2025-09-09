# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from typing import TYPE_CHECKING

# 第三方库导入
import httpx
from qfluentwidgets import (
    BodyLabel,
    ImageLabel,
    IndeterminateProgressBar,
    PrimaryPushButton,
    ProgressBar,
    SubtitleLabel,
)
from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.network.downloader import GithubDownloader, QQDownloader
from src.core.network.urls import Urls
from src.core.utils.install_func import NapCatInstall, QQInstall
from src.core.utils.path_func import PathFunc
from src.ui.common.icon import NapCatDesktopIcon, StaticIcon
from src.ui.components.info_bar import success_bar
from src.ui.components.message_box import FolderBox
from src.ui.page.unit_page.status import ButtonStatus, ProgressRingStatus, StatusLabel

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.guide_window.guide_window import GuideWindow


class InstallPageBase(QWidget):
    """安装页面基类

    Attributes:
        BUTTON_VISIBILITY (dict): 按钮显示标识符
        PROGRESS_RING_VISIBILITY (dict): 进度条显示标识符
        STATUS_LABEL_VISIBILITY (dict): 状态标签显示标识符
    """

    # 按钮显示标识符
    BUTTON_VISIBILITY: dict = {
        ButtonStatus.INSTALL: {"install": False, "update": False, "openFolder": True},
        ButtonStatus.UNINSTALLED: {"install": True, "update": False, "openFolder": False},
        ButtonStatus.UPDATE: {"install": False, "update": True, "openFolder": False},
        ButtonStatus.NONE: {"install": False, "update": False, "openFolder": False},
    }

    # 进度条显示标识符
    PROGRESS_RING_VISIBILITY: dict = {
        ProgressRingStatus.INDETERMINATE: {"indeterminate": True, "determinate": False},
        ProgressRingStatus.DETERMINATE: {"indeterminate": False, "determinate": True},
        ProgressRingStatus.NONE: {"indeterminate": False, "determinate": False},
    }

    # 状态标签显示标识符
    STATUS_LABEL_VISIBILITY: dict = {StatusLabel.SHOW: True, StatusLabel.HIDE: False}

    def __init__(self, parent: GuideWindow) -> None:
        """初始化页面

        Args:
            parent (GuideWindow): 主窗体
        """
        super().__init__(parent)

        # 创建控件
        self.title_label = SubtitleLabel(self)
        self.icon_label = ImageLabel(self)
        self.install_button = PrimaryPushButton(self.tr("安装"), self)
        self.status_label = BodyLabel(self.tr("正在安装..."), self)
        self.indeterminate_progress_ring = IndeterminateProgressBar(self)
        self.progress_bar = ProgressBar(self)

        # 设置属性
        self.status_label.hide()
        self.indeterminate_progress_ring.hide()
        self.progress_bar.hide()

        self.progress_bar.setTextVisible(True)
        self.progress_bar.setFixedWidth(200)
        self.indeterminate_progress_ring.setFixedWidth(200)
        self.install_button.setFixedWidth(175)

        # 布局
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(24)
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.title_label, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.icon_label, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.install_button, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.status_label, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.indeterminate_progress_ring, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.progress_bar, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addStretch(1)

    def set_icon(self, icon: QPixmap | str) -> None:
        """设置图标

        Args:
            icon (QPixmap | str): 图标, 可以是 QPixmap 对象或图片路径

        Raises:
            TypeError: icon 不是 QPixmap 或 str 类型
        """
        if isinstance(icon, str):
            icon = QPixmap(icon)

        elif not isinstance(icon, QPixmap):
            raise TypeError("icon must be a QPixmap or str")

        self.icon_label.setPixmap(icon)
        self.icon_label.scaledToWidth(128)

    def set_title(self, title: str) -> None:
        """设置标题

        Args:
            title (str): 标题
        """
        self.title_label.setText(title)

    def set_status_text(self, text: str) -> None:
        """设置状态标签文本

        Args:
            text (str): 文本
        """
        self.status_label.setText(text)

    def set_progress_ring_value(self, value: int) -> None:
        """设置进度环数值

        Args:
            value (int): 数值
        """
        self.progress_bar.setValue(value)

    def set_visibility(self, visible_buttons: dict, visible_progress_rings: dict, visible_status_label: dict) -> None:
        """设置按钮和进度环以及状态标签的可见性

        Args:
            visible_buttons (dict): 按钮可见性字典
            visible_progress_rings (dict): 进度环可见性字典
            visible_status_label (bool): 状态标签可见性
        """
        self.install_button.setVisible(visible_buttons.get("install", False))
        self.indeterminate_progress_ring.setVisible(visible_progress_rings.get("indeterminate", False))
        self.progress_bar.setVisible(visible_progress_rings.get("determinate", False))
        self.status_label.setVisible(visible_status_label)

    def on_switch_button(self, status: ButtonStatus) -> None:
        """切换按钮

        Args:
            status (ButtonStatus): _description_
        """
        self.set_visibility(
            self.BUTTON_VISIBILITY[status],
            self.progress_ring_visibility[ProgressRingStatus.NONE],
            self.status_label_visibility[StatusLabel.HIDE],
        )

    def on_switch_progress_ring(self, status: ProgressRingStatus) -> None:
        """切换进度环

        切换进度环, 如果需要显示进度条, 则隐藏所有按钮, 并显示 statusLabel

        Args:
            status (ProgressRingStatus): 进度环状态
        """
        self.set_visibility(
            self.BUTTON_VISIBILITY[ButtonStatus.NONE],
            self.progress_ring_visibility[status],
            self.status_label_visibility[StatusLabel.SHOW],
        )


class InstallQQPage(InstallPageBase):
    """安装 QQ 页面"""

    def __init__(self, parent: GuideWindow) -> None:
        """初始化页面

        Args:
            parent (GuideWindow): 主窗体
        """
        super().__init__(parent)
        # 创建杂七杂八的控件
        self.url = self.get_download_url()
        self.file_path = PathFunc().tmp_path / self.url.fileName()
        self.downloader = QQDownloader(self.url)

        # 设置属性
        self.set_icon(NapCatDesktopIcon.QQ.path())
        self.set_title(self.tr("安装 QQ 客户端"))

        # 信号连接
        self.install_button.clicked.connect(self.on_download)

    def get_download_url(self) -> QUrl:
        """获取最新 QQ 下载链接"""
        response = httpx.get(Urls.QQ_Version.value.url())
        ver_hash = response.json()["verHash"]
        version = response.json()["version"].replace("-", ".")
        download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
        return QUrl(download_url)

    def on_download(self) -> None:
        """下载"""
        self.downloader.download_progress_signal.connect(self.set_progress_ring_value)
        self.downloader.download_finish_signal.connect(self.on_install)
        self.downloader.status_label_signal.connect(self.set_status_text)
        self.downloader.button_toggle_signal.connect(self.on_switch_button)
        self.downloader.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.downloader.start()

    def on_install(self) -> None:
        """安装"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        folder_box = FolderBox(self.tr("选择安装路径"), GuideWindow())
        folder_box.cancelButton.hide()
        folder_box.exec()

        # 修改注册表, 让安装程序读取注册表按照路径安装
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.get_value().replace("/", "\\"))
        winreg.CloseKey(key)

        # 开始安装
        self.installer = QQInstall(self.file_path)
        self.installer.status_label_signal.connect(self.set_status_text)
        self.installer.button_toggle_signal.connect(self.on_switch_button)
        self.installer.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.installer.install_finish_signal.connect(self.on_install_finsh)

        self.installer.start()

    def on_install_finsh(self) -> None:
        """安装完成"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        success_bar(self.tr("安装完成"), parent=GuideWindow())
        GuideWindow().on_next_page()


class InstallNapCatQQPage(InstallPageBase):

    def __init__(self, parent: GuideWindow) -> None:
        super().__init__(parent)
        # 创建杂七杂八的控件
        self.url = Urls.NAPCATQQ_DOWNLOAD.value
        self.file_path = PathFunc().tmp_path / self.url.fileName()
        self.downloader = GithubDownloader(self.url)

        # 设置属性
        self.set_icon(StaticIcon.LOGO.path())
        self.set_title(self.tr("安装 NapCatQQ"))

        # 信号连接
        self.install_button.clicked.connect(self.on_download)

    def on_download(self) -> None:
        """下载"""
        self.downloader.download_progress_signal.connect(self.set_progress_ring_value)
        self.downloader.download_finish_signal.connect(self.on_install)
        self.downloader.status_label_signal.connect(self.set_status_text)
        self.downloader.button_toggle_signal.connect(self.on_switch_button)
        self.downloader.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.downloader.start()

    def on_install(self) -> None:
        """安装"""
        self.installer = NapCatInstall()
        self.installer.status_label_signal.connect(self.set_status_text)
        self.installer.button_toggle_signal.connect(self.on_switch_button)
        self.installer.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.installer.install_finish_signal.connect(self.install_finsh)
        self.installer.start()

    def install_finsh(self) -> None:
        """安装完成"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        success_bar(self.tr("安装完成"), parent=GuideWindow())
        GuideWindow().on_next_page()
