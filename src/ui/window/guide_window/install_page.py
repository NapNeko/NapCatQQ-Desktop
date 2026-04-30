# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from typing import TYPE_CHECKING

# 第三方库导入
import httpx
from creart import it
from qfluentwidgets import (
    BodyLabel,
    ImageLabel,
    IndeterminateProgressBar,
    PrimaryPushButton,
    ProgressBar,
    SubtitleLabel,
)
from PySide6.QtCore import Qt, QThreadPool, QUrl, Slot
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.network.downloader import GithubDownloader, QQDownloader
from src.core.network.urls import Urls
from src.core.home import home_version_refresh_bus
from src.core.versioning import RemoteVersionTask, VersionSnapshot
from src.core.installation.installers import NapCatInstall, QQInstall
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import summarize_path, summarize_url
from src.core.runtime.paths import PathFunc
from src.ui.common.icon import NapCatDesktopIcon, StaticIcon
from src.ui.components.info_bar import error_bar, success_bar
from src.ui.components.message_box import FolderBox
from src.ui.page.component_page.utils import ButtonStatus, ProgressRingStatus, StatusLabel

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

    def __init__(self, parent: "GuideWindow") -> None:
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

    def set_visibility(self, visible_buttons: dict, visible_progress_rings: dict, visible_status_label: bool) -> None:
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
            self.PROGRESS_RING_VISIBILITY[ProgressRingStatus.NONE],
            self.STATUS_LABEL_VISIBILITY[StatusLabel.HIDE],
        )

    def on_switch_progress_ring(self, status: ProgressRingStatus) -> None:
        """切换进度环

        切换进度环, 如果需要显示进度条, 则隐藏所有按钮, 并显示 statusLabel

        Args:
            status (ProgressRingStatus): 进度环状态
        """
        self.set_visibility(
            self.BUTTON_VISIBILITY[ButtonStatus.NONE],
            self.PROGRESS_RING_VISIBILITY[status],
            self.STATUS_LABEL_VISIBILITY[StatusLabel.SHOW],
        )

    @Slot()
    def on_error_finish(self) -> None:
        """统一处理安装引导页中的下载/安装失败。"""
        logger.error(f"引导安装页流程失败: page={type(self).__name__}", log_source=LogSource.UI)
        self.on_switch_button(ButtonStatus.UNINSTALLED)
        error_bar(self.tr("操作失败，请重试"), parent=self.window())


class InstallQQPage(InstallPageBase):
    """安装 QQ 页面"""

    def __init__(self, parent: "GuideWindow") -> None:
        """初始化页面

        Args:
            parent (GuideWindow): 主窗体
        """
        super().__init__(parent)
        self.url: QUrl | None = None
        self.file_path = None
        self.downloader: QQDownloader | None = None
        self.installer: QQInstall | None = None
        self.remote_version_task: RemoteVersionTask | None = None

        # 设置属性
        self.set_icon(NapCatDesktopIcon.QQ.path())
        self.set_title(self.tr("安装 QQ 客户端"))

        # 信号连接
        self.install_button.clicked.connect(self.handle_download_requested)
        self._fetch_download_url()

    def get_download_url(self) -> QUrl:
        """获取最新 QQ 下载链接"""
        try:
            response = httpx.get(Urls.QQ_Version.value.url())
            ver_hash = response.json()["verHash"]
            version = response.json()["version"].replace("-", ".")
            download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
            return QUrl(download_url)
        except httpx.ConnectTimeout:
            self.set_status_text(self.tr("网络连接超时，请检查您的网络连接。"))
            return QUrl()

    def _fetch_download_url(self) -> None:
        """异步获取 QQ 下载链接，避免阻塞引导窗口初始化。"""
        if self.remote_version_task is not None:
            return

        self.set_status_text(self.tr("正在获取 QQ 下载链接..."))
        self.on_switch_progress_ring(ProgressRingStatus.INDETERMINATE)
        self.remote_version_task = RemoteVersionTask()
        self.remote_version_task.version_signal.connect(self.apply_remote_version_data)
        QThreadPool.globalInstance().start(self.remote_version_task)

    @Slot(object)
    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        """接收远程版本信息，并提取 QQ 下载链接。"""
        self.remote_version_task = None
        download_url = version_data.qq_download_url
        self.on_switch_button(ButtonStatus.UNINSTALLED)

        if download_url is None:
            self.url = None
            self.file_path = None
            logger.error("引导安装 QQ: 获取下载链接失败", log_source=LogSource.UI)
            error_bar(self.tr("获取 QQ 下载链接失败，请重试"), parent=self.window())
            return

        self.url = QUrl(download_url)
        self.file_path = it(PathFunc).tmp_path / self.url.fileName()
        logger.info(
            f"引导安装 QQ: 下载链接已就绪 source={summarize_url(self.url.toString())}",
            log_source=LogSource.UI,
        )

    @Slot()
    def handle_download_requested(self) -> None:
        """下载。"""
        if self.url is None:
            logger.warning("引导安装 QQ: 下载链接未就绪，重新获取", log_source=LogSource.UI)
            self._fetch_download_url()
            return

        logger.info(
            f"引导安装 QQ: 开始下载 installer={self.url.fileName()}, source={summarize_url(self.url.toString())}",
            log_source=LogSource.UI,
        )
        self.file_path = it(PathFunc).tmp_path / self.url.fileName()
        self.downloader = QQDownloader(self.url)
        self.downloader.download_progress_signal.connect(self.set_progress_ring_value)
        self.downloader.download_finish_signal.connect(self.handle_install_requested)
        self.downloader.status_label_signal.connect(self.set_status_text)
        self.downloader.error_finsh_signal.connect(self.on_error_finish)
        self.downloader.button_toggle_signal.connect(self.on_switch_button)
        self.downloader.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        QThreadPool.globalInstance().start(self.downloader)

    @Slot()
    def handle_install_requested(self) -> None:
        """安装。"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        folder_box = FolderBox(self.tr("选择安装路径"), it(GuideWindow))
        folder_box.cancelButton.hide()
        folder_box.exec()
        logger.info("引导安装 QQ: 已确认安装路径", log_source=LogSource.UI)

        # 修改注册表, 让安装程序读取注册表按照路径安装
        key = winreg.CreateKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        winreg.SetValueEx(key, "Install", 0, winreg.REG_SZ, folder_box.get_value().replace("/", "\\"))
        winreg.CloseKey(key)

        # 开始安装
        if self.file_path is None:
            logger.error("引导安装 QQ: 安装包路径为空", log_source=LogSource.UI)
            self.on_error_finish()
            return

        self.installer = QQInstall(self.file_path)
        self.installer.status_label_signal.connect(self.set_status_text)
        self.installer.error_finish_signal.connect(self.on_error_finish)
        self.installer.button_toggle_signal.connect(self.on_switch_button)
        self.installer.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.installer.install_finish_signal.connect(self.handle_install_finished)

        QThreadPool.globalInstance().start(self.installer)

    @Slot()
    def handle_install_finished(self) -> None:
        """安装完成。"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        logger.info(
            f"引导安装 QQ 完成: installer={summarize_path(self.file_path) if self.file_path else '<empty-path>'}",
            log_source=LogSource.UI,
        )
        home_version_refresh_bus.request_refresh()
        success_bar(self.tr("安装完成"), parent=it(GuideWindow))
        it(GuideWindow).on_next_page()


class InstallNapCatQQPage(InstallPageBase):

    def __init__(self, parent: "GuideWindow") -> None:
        super().__init__(parent)
        # 创建杂七杂八的控件
        self.url = Urls.NAPCATQQ_DOWNLOAD.value
        self.file_path = it(PathFunc).tmp_path / self.url.fileName()
        self.downloader: GithubDownloader | None = None
        self.installer: NapCatInstall | None = None

        # 设置属性
        self.set_icon(StaticIcon.LOGO.path())
        self.set_title(self.tr("安装 NapCatQQ"))

        # 信号连接
        self.install_button.clicked.connect(self.handle_download_requested)

    @Slot()
    def handle_download_requested(self) -> None:
        """下载。"""
        logger.info(
            f"引导安装 NapCat: 开始下载 package={self.url.fileName()}, source={summarize_url(self.url.toString())}",
            log_source=LogSource.UI,
        )
        self.downloader = GithubDownloader(self.url)
        self.downloader.download_progress_signal.connect(self.set_progress_ring_value)
        self.downloader.download_finish_signal.connect(self.handle_install_requested)
        self.downloader.status_label_signal.connect(self.set_status_text)
        self.downloader.error_finsh_signal.connect(self.on_error_finish)
        self.downloader.button_toggle_signal.connect(self.on_switch_button)
        self.downloader.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)

        QThreadPool.globalInstance().start(self.downloader)

    @Slot()
    def handle_install_requested(self) -> None:
        """安装。"""
        logger.info("引导安装 NapCat: 下载完成，开始安装", log_source=LogSource.UI)
        self.installer = NapCatInstall()
        self.installer.status_label_signal.connect(self.set_status_text)
        self.installer.error_finish_signal.connect(self.on_error_finish)
        self.installer.button_toggle_signal.connect(self.on_switch_button)
        self.installer.progress_ring_toggle_signal.connect(self.on_switch_progress_ring)
        self.installer.install_finish_signal.connect(self.handle_install_finished)

        QThreadPool.globalInstance().start(self.installer)

    @Slot()
    def handle_install_finished(self) -> None:
        """安装完成。"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        logger.info(
            f"引导安装 NapCat 完成: path={summarize_path(it(PathFunc).napcat_path)}",
            log_source=LogSource.UI,
        )
        home_version_refresh_bus.request_refresh()
        success_bar(self.tr("安装完成"), parent=it(GuideWindow))
        it(GuideWindow).on_next_page()

