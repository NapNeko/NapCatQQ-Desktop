# -*- coding: utf-8 -*-
"""Unit 页面内部使用的一些基类 Widget"""
# 标准库导入
import re
from typing import Dict, Union

# 第三方库导入
from markdown import markdown
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    HeaderCardWidget,
    HyperlinkLabel,
    ImageLabel,
    IndeterminateProgressRing,
    PrimaryPushButton,
    ProgressRing,
    PushButton,
    ScrollArea,
    SimpleCardWidget,
    TitleLabel,
    TransparentToolButton,
    setFont,
)
from PySide6.QtCore import QSize, Qt, QUrl
from PySide6.QtGui import QDesktopServices, QFont, QImage, QPixmap
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.ui.common.icon import StaticIcon
from src.ui.components.code_editor import UpdateLogExhibit
from src.ui.page.unit_page.status import ButtonStatus, ProgressRingStatus, StatusLabel


class PageBase(ScrollArea):
    """Unit 页面的基础布局类, 包含左侧展示卡片和右侧更新日志卡片"""

    def __init__(self, parent) -> None:
        """初始化页面基础布局

        Args:
            parent: 父级控件
        """
        super().__init__(parent)

        # 创建控件
        self.app_card = DisplayCard(self)
        self.log_card = UpdateLogCard(self)

        # 进行布局
        self.h_box_layout = QHBoxLayout()
        self.h_box_layout.addWidget(self.app_card, 1)
        self.h_box_layout.addWidget(self.log_card, 2)
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.h_box_layout)
        self.get_version = parent.get_version


class DisplayCard(SimpleCardWidget):
    """左侧的应用展示卡片, 包含图标、名称、状态和操作按钮"""

    # 按钮显示标识符
    BUTTON_VISIBILITY: Dict[ButtonStatus, Dict[str, bool]] = {
        ButtonStatus.INSTALL: {"install": False, "update": False, "openFolder": True},
        ButtonStatus.UNINSTALLED: {"install": True, "update": False, "openFolder": False},
        ButtonStatus.UPDATE: {"install": False, "update": True, "openFolder": False},
        ButtonStatus.NONE: {"install": False, "update": False, "openFolder": False},
    }

    # 进度条显示标识符
    PROGRESS_RING_VISIBILITY: Dict[ProgressRingStatus, Dict[str, bool]] = {
        ProgressRingStatus.INDETERMINATE: {"indeterminate": True, "determinate": False},
        ProgressRingStatus.DETERMINATE: {"indeterminate": False, "determinate": True},
        ProgressRingStatus.NONE: {"indeterminate": False, "determinate": False},
    }

    # 状态标签显示标识符
    STATUS_LABEL_VISIBILITY: Dict[StatusLabel, bool] = {StatusLabel.SHOW: True, StatusLabel.HIDE: False}

    def __init__(self, parent) -> None:
        """初始化展示卡片

        Args:
            parent: 父级控件
        """
        super().__init__(parent)

        # 创建控件
        self.icon_label = ImageLabel(StaticIcon.LOGO.path(), self)
        self.name_label = TitleLabel("Unknown", self)
        self.hyper_label = HyperlinkLabel("Unknown", self)
        self.status_label = BodyLabel("Unknown", self)

        self.install_button = PrimaryPushButton(self.tr("安装"), self)
        self.update_button = PrimaryPushButton(self.tr("更新"), self)
        self.open_folder_button = PushButton(self.tr("打开文件夹"), self)

        self.indeterminate_progress_ring = IndeterminateProgressRing(self)
        self.progress_ring = ProgressRing(self)

        self.v_box_layout = QVBoxLayout()

        # 设置控件属性
        self._setup_widget_properties()

        # 隐藏初始控件
        self._hide_initial_widgets()

        # 设置布局
        self._setup_layout()

    def _setup_widget_properties(self) -> None:
        """配置控件的初始属性"""
        self.setMaximumWidth(400)
        self.setMinimumWidth(230)

        self.icon_label.setBorderRadius(8, 8, 8, 8)
        self.icon_label.scaledToHeight(128)

        self.install_button.setMinimumWidth(140)
        self.update_button.setMinimumWidth(140)
        self.open_folder_button.setMinimumWidth(140)

        self.indeterminate_progress_ring.setFixedSize(QSize(72, 72))
        self.progress_ring.setFixedSize(QSize(72, 72))
        self.progress_ring.setTextVisible(True)

        setFont(self.name_label, 22, QFont.Weight.Bold)
        setFont(self.status_label, 13, QFont.Weight.Normal)

    def _hide_initial_widgets(self) -> None:
        """隐藏初始不需要显示的控件"""
        self.install_button.hide()
        self.update_button.hide()
        self.status_label.hide()
        self.indeterminate_progress_ring.hide()
        self.progress_ring.hide()

    def _setup_layout(self) -> None:
        """设置卡片布局"""
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addStretch(3)
        self.v_box_layout.addWidget(self.icon_label, 1, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.addWidget(self.name_label, 1, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addSpacing(5)
        self.v_box_layout.addWidget(self.hyper_label, 1, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addStretch(3)
        self.v_box_layout.addWidget(self.install_button, 2, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.update_button, 2, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.open_folder_button, 2, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.indeterminate_progress_ring, 2, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addWidget(self.progress_ring, 2, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.addWidget(self.status_label, 0, Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addStretch(3)

        self.setLayout(self.v_box_layout)

    # ==================== 公共方法 ====================
    def set_icon(self, icon: Union[str, QPixmap, QImage]) -> None:
        """设置应用图标

        Args:
            icon: 图标路径或图标对象
        """
        self.icon_label.setImage(icon)

    def set_name(self, name: str) -> None:
        """设置应用名称

        Args:
            name: 应用名称
        """
        self.name_label.setText(name)

    def set_hyper_label_name(self, name: str) -> None:
        """设置超链接标签文本

        Args:
            name: 超链接显示文本
        """
        self.hyper_label.setText(name)

    def set_hyper_label_url(self, url: Union[str, QUrl]) -> None:
        """设置超链接URL

        Args:
            url: 超链接URL地址
        """
        self.hyper_label.setUrl(url)

    def set_status_text(self, text: str) -> None:
        """设置状态标签文本

        Args:
            text: 状态文本
        """
        self.status_label.setText(text)

    def set_progress_ring_value(self, value: int) -> None:
        """设置进度环数值

        Args:
            value: 进度值(0-100)
        """
        self.progress_ring.setValue(value)

    def set_visibility(
        self, visible_buttons: Dict[str, bool], visible_progress_rings: Dict[str, bool], visible_status_label: bool
    ) -> None:
        """设置按钮、进度环和状态标签的可见性

        Args:
            visible_buttons: 按钮可见性字典
            visible_progress_rings: 进度环可见性字典
            visible_status_label: 状态标签可见性
        """
        self.install_button.setVisible(visible_buttons.get("install", False))
        self.update_button.setVisible(visible_buttons.get("update", False))
        self.open_folder_button.setVisible(visible_buttons.get("openFolder", False))

        self.indeterminate_progress_ring.setVisible(visible_progress_rings.get("indeterminate", False))
        self.progress_ring.setVisible(visible_progress_rings.get("determinate", False))

        self.status_label.setVisible(visible_status_label)

    def switch_button(self, status: ButtonStatus) -> None:
        """切换按钮显示状态, 显示按钮时隐藏所有进度条

        Args:
            status: 按钮状态枚举
        """
        self.set_visibility(
            self.BUTTON_VISIBILITY[status],
            self.PROGRESS_RING_VISIBILITY[ProgressRingStatus.NONE],
            self.STATUS_LABEL_VISIBILITY[StatusLabel.HIDE],
        )

    def switch_progress_ring(self, status: ProgressRingStatus) -> None:
        """切换进度环显示状态, 显示进度条时隐藏所有按钮

        Args:
            status: 进度环状态枚举
        """
        self.set_visibility(
            self.BUTTON_VISIBILITY[ButtonStatus.NONE],
            self.PROGRESS_RING_VISIBILITY[status],
            self.STATUS_LABEL_VISIBILITY[StatusLabel.SHOW],
        )


class UpdateLogCard(HeaderCardWidget):
    """右侧的更新日志展示卡片"""

    def __init__(self, parent) -> None:
        """初始化更新日志卡片

        Args:
            parent: 父级控件
        """
        super().__init__(parent)

        # 创建属性
        self.url = QUrl()

        # 创建控件
        self.log_edit = UpdateLogExhibit(self)
        self.url_button = TransparentToolButton(FluentIcon.GLOBE)

        # 设置控件属性
        self._setup_widget_properties()

        # 设置布局
        self._setup_layout()

    def _setup_widget_properties(self) -> None:
        """配置控件属性"""
        self.setTitle(self.tr("更新日志"))
        self.url_button.clicked.connect(lambda: QDesktopServices.openUrl(self.url))

    def _setup_layout(self) -> None:
        """设置卡片布局"""
        self.headerLayout.addWidget(self.url_button, 0, Qt.AlignmentFlag.AlignRight)
        self.viewLayout.addWidget(self.log_edit)
        self.viewLayout.setContentsMargins(8, 4, 8, 4)

    # ==================== 公共方法 ====================
    def setLog(self, text: str) -> None:
        """设置更新日志内容, 支持Markdown格式

        Args:
            text: Markdown格式的文本内容
        """
        css_style = "<style>pre { white-space: pre-wrap; }</style>"
        formatted_text = re.sub(r"\r\n", "\n", text)
        self.log_edit.setHtml(css_style + markdown(formatted_text, extensions=["nl2br"]))

    def set_url(self, url: str) -> None:
        """设置外部链接URL

        Args:
            url: 链接地址
        """
        self.url.setUrl(url)
