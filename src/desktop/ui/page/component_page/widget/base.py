# -*- coding: utf-8 -*-
"""组件页面内部使用的一些基类 Widget。"""
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
from PySide6.QtCore import QSize, Qt, QUrl
from PySide6.QtGui import QColor, QDesktopServices, QFont, QImage, QPixmap
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.desktop.ui.common.icon import StaticIcon
from src.desktop.ui.components.code_editor import UpdateLogExhibit
from src.desktop.ui.components.skeleton_widget import SkeletonShape, SkeletonWidget
from src.desktop.ui.components.stacked_widget import TransparentStackedWidget
from ..utils import ButtonStatus, ProgressRingStatus, StatusLabel


class PageBase(ScrollArea):
    """组件页面基础布局，包含展示卡片和更新日志卡片。"""

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
        self.version_service = parent.version_service

        self.local_version = None
        self.remote_version = None
        self._local_version_loaded = False
        self._remote_version_loaded = False
        self._operation_in_progress = False
        self._operation_paused = False
        self._operation_controls_visible = False
        self._operation_status_text = ""
        self._operation_progress_value = 0
        self._operation_progress_status = ProgressRingStatus.NONE

    def begin_version_refresh(self) -> None:
        """标记一次新的版本刷新开始，避免并发回调使用上一次的缓存结果。"""
        self._local_version_loaded = False
        self._remote_version_loaded = False

    def mark_local_version_loaded(self) -> None:
        """记录本地版本已完成读取。"""
        self._local_version_loaded = True

    def mark_remote_version_loaded(self) -> None:
        """记录远程版本已完成读取。"""
        self._remote_version_loaded = True

    def refresh_page_if_ready(self) -> None:
        """仅在本地与远程版本都返回后再刷新页面，避免竞态导致按钮状态错误。"""
        if self._local_version_loaded and self._remote_version_loaded:
            self.refresh_page_view()

    def is_operation_in_progress(self) -> bool:
        """当前页面是否仍有下载/安装任务正在执行。"""
        return self._operation_in_progress

    def is_operation_paused(self) -> bool:
        """当前下载是否处于暂停状态。"""
        return self._operation_in_progress and self._operation_paused

    def begin_download_operation(self, status_text: str | None = None) -> None:
        """进入可暂停/取消的下载状态。"""
        self.begin_operation(status_text=status_text)
        self._operation_paused = False
        self._operation_controls_visible = True
        self.restore_operation_view()

    def begin_install_operation(self, status_text: str | None = None) -> None:
        """进入安装状态，隐藏下载控制按钮。"""
        self.begin_operation(status_text=status_text)
        self._operation_paused = False
        self._operation_controls_visible = False
        self.restore_operation_view()

    def begin_operation(self, status_text: str | None = None) -> None:
        """进入操作中状态，并锁定页面按钮显示。"""
        self._operation_in_progress = True
        self._operation_progress_value = 0
        self._operation_progress_status = ProgressRingStatus.INDETERMINATE
        self._operation_paused = False
        if status_text is not None:
            self._operation_status_text = status_text
        self.restore_operation_view()

    def pause_operation(self, status_text: str | None = None) -> None:
        """将当前下载状态切换为已暂停。"""
        self._operation_in_progress = True
        self._operation_paused = True
        self._operation_controls_visible = True
        if status_text is not None:
            self._operation_status_text = status_text
        self.restore_operation_view()

    def resume_operation(self, status_text: str | None = None) -> None:
        """恢复暂停中的下载状态。"""
        self._operation_in_progress = True
        self._operation_paused = False
        self._operation_controls_visible = True
        if status_text is not None:
            self._operation_status_text = status_text
        self.restore_operation_view()

    def update_operation_status_text(self, text: str) -> None:
        """更新当前操作的状态文本。"""
        self._operation_status_text = text
        self.app_card.set_status_text(text)

    def update_operation_progress_value(self, value: int) -> None:
        """更新当前操作的进度值。"""
        self._operation_progress_value = value
        self.app_card.set_progress_ring_value(value)

    def update_operation_progress_ring(self, status: ProgressRingStatus) -> None:
        """更新当前操作的进度环模式。"""
        self._operation_progress_status = status
        if self._operation_controls_visible:
            self.app_card.switch_download_controls(status=status, paused=self._operation_paused)
            return

        self.app_card.switch_progress_ring(status)

    def end_operation(self) -> None:
        """结束当前下载/安装操作状态。"""
        self._operation_in_progress = False
        self._operation_paused = False
        self._operation_controls_visible = False
        self._operation_status_text = ""
        self._operation_progress_value = 0
        self._operation_progress_status = ProgressRingStatus.NONE

    def restore_operation_view(self) -> bool:
        """当页面刷新时恢复正在执行中的操作视图。"""
        if not self._operation_in_progress:
            return False

        if self._operation_status_text:
            self.app_card.set_status_text(self._operation_status_text)
        self.app_card.set_progress_ring_value(self._operation_progress_value)

        status = self._operation_progress_status
        if status == ProgressRingStatus.NONE:
            status = ProgressRingStatus.INDETERMINATE
        if self._operation_controls_visible:
            self.app_card.switch_download_controls(status=status, paused=self._operation_paused)
        else:
            self.app_card.switch_progress_ring(status)
        return True


class UpdateLogSkeleton(SkeletonWidget):
    """更新日志加载骨架屏。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(self._build_shapes, parent, panel_margin=4, panel_radius=18)
        self.setMinimumWidth(320)

    def _base_color(self) -> QColor:
        return QColor(255, 255, 255, 22) if isDarkTheme() else QColor(15, 23, 42, 14)

    def _highlight_color(self) -> QColor:
        return QColor(255, 255, 255, 52) if isDarkTheme() else QColor(255, 255, 255, 108)

    def _build_shapes(self, widget: QWidget) -> list[SkeletonShape]:
        panel_rect = widget.rect().adjusted(8, 10, -8, -10)
        if panel_rect.width() <= 0 or panel_rect.height() <= 0:
            return []
        x = panel_rect.x() + 14
        y = panel_rect.y() + 12
        width = panel_rect.width() - 28
        shapes: list[SkeletonShape] = []

        shapes.append(SkeletonShape(x, y, int(width * 0.46), 18, 1.08))
        y += 30

        chip_gap = 10
        chip_widths = [82, 108, 74]
        for chip_width in chip_widths:
            shapes.append(SkeletonShape(x, y, chip_width, 12, 1.0))
            x += chip_width + chip_gap

        x = panel_rect.x() + 14
        y += 28

        shapes.append(SkeletonShape(x, y, int(width * 0.94), 72, 1.22, 18))
        y += 92

        section_widths = [0.64, 0.88, 0.77, 0.52]
        shapes.append(SkeletonShape(x, y, int(width * 0.34), 16, 1.04))
        y += 28
        for ratio in section_widths:
            shapes.append(SkeletonShape(x, y, int(width * ratio), 12, 1.02))
            y += 20

        y += 16
        shapes.append(SkeletonShape(x, y, int(width * 0.28), 16, 1.04))
        y += 28

        left_col_width = int(width * 0.44)
        right_col_width = int(width * 0.38)
        card_height = 84
        shapes.append(SkeletonShape(x, y, left_col_width, card_height, 1.14, 14))
        shapes.append(SkeletonShape(x + left_col_width + 16, y, right_col_width, card_height, 1.1, 14))

        y += card_height + 24
        footer_widths = [0.90, 0.73, 0.82]
        for ratio in footer_widths:
            shapes.append(SkeletonShape(x, y, int(width * ratio), 12, 1.0))
            y += 18

        return shapes


class DisplayCard(SimpleCardWidget):
    """左侧的应用展示卡片, 包含图标、名称、状态和操作按钮"""

    # 按钮显示标识符
    BUTTON_VISIBILITY: Dict[ButtonStatus, Dict[str, bool]] = {
        ButtonStatus.INSTALL: {"install": False, "update": False, "openFolder": True, "pause": False, "cancel": False},
        ButtonStatus.UNINSTALLED: {"install": True, "update": False, "openFolder": False, "pause": False, "cancel": False},
        ButtonStatus.UPDATE: {"install": False, "update": True, "openFolder": False, "pause": False, "cancel": False},
        ButtonStatus.NONE: {"install": False, "update": False, "openFolder": False, "pause": False, "cancel": False},
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
        self.pause_button = PushButton(self.tr("暂停"), self)
        self.cancel_button = PushButton(self.tr("取消"), self)

        self.indeterminate_progress_ring = IndeterminateProgressRing(self)
        self.progress_ring = ProgressRing(self)

        self.v_box_layout = QVBoxLayout()
        self.download_control_layout = QHBoxLayout()

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
        self.pause_button.setMinimumWidth(140)
        self.cancel_button.setMinimumWidth(140)

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
        self.pause_button.hide()
        self.cancel_button.hide()

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
        self.v_box_layout.addSpacing(12)

        self.download_control_layout.setContentsMargins(0, 0, 0, 0)
        self.download_control_layout.setSpacing(12)
        self.download_control_layout.setAlignment(Qt.AlignmentFlag.AlignHCenter)
        self.download_control_layout.addWidget(self.pause_button)
        self.download_control_layout.addWidget(self.cancel_button)
        self.v_box_layout.addLayout(self.download_control_layout)

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
        self.pause_button.setVisible(visible_buttons.get("pause", False))
        self.cancel_button.setVisible(visible_buttons.get("cancel", False))

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

    def switch_download_controls(self, status: ProgressRingStatus, paused: bool = False) -> None:
        """显示下载中的控制按钮，并保留进度展示。"""
        self.pause_button.setText(self.tr("继续") if paused else self.tr("暂停"))
        self.set_visibility(
            {"install": False, "update": False, "openFolder": False, "pause": True, "cancel": True},
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

    def set_log_markdown(self, text: str) -> None:
        """设置更新日志内容，支持 Markdown 格式。

        Args:
            text: Markdown格式的文本内容
        """
        css_style = "<style>pre { white-space: pre-wrap; }</style>"
        self.log_edit.setHtml(css_style + markdown(text, extensions=["nl2br"]))
        self.set_loading(False)

    def setLog(self, text: str) -> None:
        """兼容旧命名。"""
        self.set_log_markdown(text)

    def set_url(self, url: str) -> None:
        """设置外部链接URL

        Args:
            url: 链接地址
        """
        self.url.setUrl(url)
