# -*- coding: utf-8 -*-
"""Unit 页面内部使用的一些基类 Widget"""
# 标准库导入
import re
from typing import Dict, Union

# 第三方库导入
from markdown import markdown
from qfluentwidgets.common.overload import singledispatchmethod
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
    isDarkTheme,
    setFont,
)
from PySide6.QtCore import QSize, Qt, QTimer, QUrl
from PySide6.QtGui import QColor, QDesktopServices, QFont, QImage, QLinearGradient, QPainter, QPaintEvent, QPixmap
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.ui.common.icon import StaticIcon
from src.ui.components.code_editor import UpdateLogExhibit
from src.ui.components.stacked_widget import TransparentStackedWidget
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

        self.local_version = None
        self.remote_version = None


class UpdateLogSkeleton(QWidget):
    """更新日志加载骨架屏。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._phase = 0.0
        self._timer = QTimer(self)
        self._timer.setInterval(32)
        self._timer.timeout.connect(self._advance_phase)

        self.setMinimumWidth(320)
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, False)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)

    def showEvent(self, event) -> None:
        super().showEvent(event)
        if not self._timer.isActive():
            self._timer.start()

    def hideEvent(self, event) -> None:
        self._timer.stop()
        super().hideEvent(event)

    def _advance_phase(self) -> None:
        self._phase += 0.018
        if self._phase > 1.0:
            self._phase = 0.0
        self.update()

    def _base_color(self) -> QColor:
        return QColor(255, 255, 255, 20) if isDarkTheme() else QColor(15, 23, 42, 12)

    def _highlight_color(self) -> QColor:
        return QColor(255, 255, 255, 56) if isDarkTheme() else QColor(255, 255, 255, 150)

    def _panel_color(self) -> QColor:
        return QColor(255, 255, 255, 10) if isDarkTheme() else QColor(255, 255, 255, 188)

    def _outline_color(self) -> QColor:
        return QColor(255, 255, 255, 16) if isDarkTheme() else QColor(15, 23, 42, 10)

    def _draw_line(self, painter: QPainter, x: int, y: int, width: int, height: int) -> None:
        radius = height / 2
        gradient = self._create_shimmer_gradient(x, y, width, band_scale=0.54)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(gradient)
        painter.drawRoundedRect(x, y, width, height, radius, radius)

    def _draw_block(self, painter: QPainter, x: int, y: int, width: int, height: int, radius: int = 14) -> None:
        gradient = self._create_shimmer_gradient(x, y, width, band_scale=0.62)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(gradient)
        painter.drawRoundedRect(x, y, width, height, radius, radius)

    def _create_shimmer_gradient(self, x: int, y: int, width: int, *, band_scale: float) -> QLinearGradient:
        """创建横向移动的 shimmer 渐变，高光带会在可视区外完成重置。"""
        band_width = max(96.0, width * band_scale)
        travel = width + band_width * 2
        start_x = x - band_width + travel * self._phase
        gradient = QLinearGradient(start_x, y, start_x + band_width, y)

        gradient.setColorAt(0.0, self._base_color())
        gradient.setColorAt(0.32, self._base_color())
        gradient.setColorAt(0.5, self._highlight_color())
        gradient.setColorAt(0.68, self._base_color())
        gradient.setColorAt(1.0, self._base_color())
        return gradient

    def paintEvent(self, event: QPaintEvent) -> None:
        super().paintEvent(event)

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        margin = 14
        panel_rect = self.rect().adjusted(margin, margin, -margin, -margin)
        if panel_rect.width() <= 0 or panel_rect.height() <= 0:
            painter.end()
            return

        painter.setPen(self._outline_color())
        painter.setBrush(self._panel_color())
        painter.drawRoundedRect(panel_rect, 18, 18)

        x = panel_rect.x() + 20
        y = panel_rect.y() + 18
        width = panel_rect.width() - 40

        self._draw_line(painter, x, y, int(width * 0.46), 18)
        y += 30

        chip_gap = 10
        chip_widths = [82, 108, 74]
        for chip_width in chip_widths:
            self._draw_line(painter, x, y, chip_width, 12)
            x += chip_width + chip_gap

        x = panel_rect.x() + 20
        y += 28

        self._draw_block(painter, x, y, int(width * 0.92), 72, radius=18)
        y += 92

        section_widths = [0.64, 0.88, 0.77, 0.52]
        self._draw_line(painter, x, y, int(width * 0.34), 16)
        y += 28
        for ratio in section_widths:
            self._draw_line(painter, x, y, int(width * ratio), 12)
            y += 20

        y += 16
        self._draw_line(painter, x, y, int(width * 0.28), 16)
        y += 28

        left_col_width = int(width * 0.44)
        right_col_width = int(width * 0.38)
        card_height = 84
        self._draw_block(painter, x, y, left_col_width, card_height)
        self._draw_block(painter, x + left_col_width + 16, y, right_col_width, card_height)

        y += card_height + 24
        footer_widths = [0.90, 0.73, 0.82]
        for ratio in footer_widths:
            self._draw_line(painter, x, y, int(width * ratio), 12)
            y += 18

        painter.end()


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

    @singledispatchmethod
    def __init__(self, parent) -> None:
        """初始化更新日志卡片

        Args:
            parent: 父级控件
        """
        super().__init__(parent)

        # 创建属性
        self.url = QUrl()

        # 创建控件
        self.content_stack = TransparentStackedWidget(self, delta_y=10, duration=240)
        self.log_edit = UpdateLogExhibit(self)
        self.skeleton = UpdateLogSkeleton(self)
        self.url_button = TransparentToolButton(FluentIcon.GLOBE)

        # 设置控件属性
        self._setup_widget_properties()

        # 设置布局
        self._setup_layout()

    def _setup_widget_properties(self) -> None:
        """配置控件属性"""
        self.setTitle(self.tr("更新日志"))
        self.url_button.clicked.connect(lambda: QDesktopServices.openUrl(self.url))
        self.content_stack.setObjectName("update_log_content_stack")
        self.content_stack.setStyleSheet("background: transparent; border: none;")
        self.content_stack.addWidget(self.skeleton)
        self.content_stack.addWidget(self.log_edit)
        self.content_stack.setCurrentWidget(self.log_edit)

    def _setup_layout(self) -> None:
        """设置卡片布局"""
        self.headerLayout.addWidget(self.url_button, 0, Qt.AlignmentFlag.AlignRight)
        self.viewLayout.addWidget(self.content_stack)
        self.viewLayout.setContentsMargins(8, 4, 8, 4)

    # ==================== 公共方法 ====================
    def set_loading(self, is_loading: bool) -> None:
        """切换更新日志的加载态。"""
        self.content_stack.setCurrentWidget(self.skeleton if is_loading else self.log_edit)

    def setLog(self, text: str) -> None:
        """设置更新日志内容, 支持Markdown格式

        Args:
            text: Markdown格式的文本内容
        """
        css_style = "<style>pre { white-space: pre-wrap; }</style>"
        self.log_edit.setHtml(css_style + markdown(text, extensions=["nl2br"]))
        self.set_loading(False)

    def set_url(self, url: str) -> None:
        """设置外部链接URL

        Args:
            url: 链接地址
        """
        self.url.setUrl(url)
