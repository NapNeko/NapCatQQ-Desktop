# -*- coding: utf-8 -*-

# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import BodyLabel, CaptionLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import IconWidget, ImageLabel, SubtitleLabel, isDarkTheme
from PySide6.QtCore import Property, QEasingCurve, QEvent, QPropertyAnimation, Qt
from PySide6.QtGui import QColor, QDesktopServices, QEnterEvent, QPainter, QPainterPath, QPaintEvent
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.network.urls import Urls
from src.ui.common.emoticons import FuFuEmoticons
from src.ui.components.separator import Separator

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.guide_window.guide_window import GuideWindow


class FinshPage(QWidget):
    """安装完成页面"""

    def __init__(self, parent: "GuideWindow") -> None:
        super().__init__(parent)

        # 创建控件
        self.title_label = SubtitleLabel(self.tr("安装完成, 下一步?"), self)
        self.fufu_emoticons_Label = ImageLabel(FuFuEmoticons.FU_13.path(), self)
        self.separator = Separator(self)
        self.document_card = DocumentCard(self)
        self.next_card = NextCard(self)

        # 设置控件属性
        self.fufu_emoticons_Label.scaledToHeight(128)

        # 设置布局
        self.card_layout = QVBoxLayout()
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(16)
        self.card_layout.addWidget(self.document_card)
        self.card_layout.addWidget(self.next_card)

        self.h_box_layout = QHBoxLayout()
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)
        self.h_box_layout.setSpacing(16)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addWidget(self.fufu_emoticons_Label)
        self.h_box_layout.addWidget(self.separator)
        self.h_box_layout.addLayout(self.card_layout)
        self.h_box_layout.addStretch(1)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(16, 16, 16, 16)
        self.v_box_layout.setSpacing(16)
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.title_label, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.v_box_layout.addSpacing(32)
        self.v_box_layout.addLayout(self.h_box_layout)
        self.v_box_layout.addStretch(1)

        # 设置属性
        self.separator.setFixedHeight(128)


class Card(QWidget):
    """卡片基类，带有悬停动画效果"""

    def __init__(self, title: str, subtitle: str, parent=None) -> None:
        super().__init__(parent)

        # 动画属性
        self._radius = 5
        self._scale = 1.0
        if isDarkTheme():
            self._background_color = QColor(48, 48, 48)
        else:
            self._background_color = QColor(240, 240, 240)

        # 创建控件
        self.title_label = BodyLabel(title, self)
        self.subtitle_label = CaptionLabel(subtitle, self)
        self.icon_label = IconWidget(FI.RIGHT_ARROW, self)

        # 设置属性
        self.setFixedSize(260, 60)
        self.icon_label.setFixedSize(12, 12)

        # 设置鼠标跟踪
        self.setMouseTracking(True)

        # 布局
        self.label_layout = QVBoxLayout()
        self.label_layout.setContentsMargins(0, 0, 0, 0)
        self.label_layout.setSpacing(8)
        self.label_layout.addWidget(self.title_label)
        self.label_layout.addWidget(self.subtitle_label)

        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.setContentsMargins(12, 12, 12, 12)
        self.h_box_layout.setSpacing(0)
        self.h_box_layout.addLayout(self.label_layout)
        self.h_box_layout.addWidget(
            self.icon_label,
            alignment=Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
        )

        # 初始化动画
        self.init_animations()

    def enterEvent(self, event: QEnterEvent):
        """重写鼠标进入事件

        当鼠标进入卡片时，启动悬停动画

        Args:
            event (QEnterEvent): _QEnterEvent_
        """
        # 鼠标进入时启动动画
        self.radius_anim.setStartValue(self._radius)
        self.radius_anim.setEndValue(10)

        self.scale_anim.setStartValue(self._scale)
        self.scale_anim.setEndValue(1.02)

        self.color_anim.setStartValue(self._background_color)
        self.color_anim.setEndValue(QColor(220, 220, 220) if not isDarkTheme() else QColor(70, 70, 70))

        self.radius_anim.start()
        self.scale_anim.start()
        self.color_anim.start()

        super().enterEvent(event)

    def leaveEvent(self, event: QEvent):
        """重写鼠标离开事件

        当鼠标离开卡片时，恢复初始状态

        Args:
            event (QEvent): _QEvent_
        """
        # 鼠标离开时恢复动画
        self.radius_anim.setStartValue(self._radius)
        self.radius_anim.setEndValue(5)

        self.scale_anim.setStartValue(self._scale)
        self.scale_anim.setEndValue(1.0)

        self.color_anim.setStartValue(self._background_color)
        self.color_anim.setEndValue(QColor(240, 240, 240) if not isDarkTheme() else QColor(48, 48, 48))

        self.radius_anim.start()
        self.scale_anim.start()
        self.color_anim.start()

        super().leaveEvent(event)

    def paintEvent(self, event: QPaintEvent):
        """重写绘制事件

        通过 QPainter 绘制圆角矩形背景，并应用当前的动画属性

        Args:
            event (QPaintEvent): _QPaintEvent_
        """
        # 绘制圆角矩形背景
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)

        path = QPainterPath()
        rect = self.rect()

        # 应用缩放
        if self._scale != 1.0:
            painter.translate(rect.center())
            painter.scale(self._scale, self._scale)
            painter.translate(-rect.center())

        path.addRoundedRect(rect, self._radius, self._radius)

        painter.fillPath(path, self._background_color)

    def init_animations(self):
        """初始化动画

        初始化用于圆角、缩放和颜色变化的动画
        """
        # 圆角动画
        self.radius_anim = QPropertyAnimation(self, b"radius")
        self.radius_anim.setDuration(300)
        self.radius_anim.setEasingCurve(QEasingCurve.OutQuad)

        # 缩放动画
        self.scale_anim = QPropertyAnimation(self, b"scale")
        self.scale_anim.setDuration(300)
        self.scale_anim.setEasingCurve(QEasingCurve.OutQuad)

        # 颜色动画
        self.color_anim = QPropertyAnimation(self, b"background_color")
        self.color_anim.setDuration(300)
        self.color_anim.setEasingCurve(QEasingCurve.OutQuad)

    # 自定义属性，用于动画
    def get_radius(self) -> int:
        return self._radius

    def set_radius(self, radius: int) -> None:
        self._radius = radius
        self.update()

    def get_scale(self) -> float:
        return self._scale

    def set_scale(self, scale: float) -> None:
        self._scale = scale
        self.update()

    def get_background_color(self) -> QColor:
        return self._background_color

    def set_background_color(self, color: QColor) -> None:
        self._background_color = color
        self.update()

    # 定义属性
    radius = Property(int, get_radius, set_radius)
    scale = Property(float, get_scale, set_scale)
    background_color = Property(QColor, get_background_color, set_background_color)


class DocumentCard(Card):
    """文档卡片"""

    def __init__(self, parent: FinshPage) -> None:
        super().__init__(parent.tr("下一步该干嘛?"), parent.tr("查看官方使用文档"), parent)

    def mousePressEvent(self, event):
        """鼠标点击事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            QDesktopServices.openUrl(Urls.NAPCATQQ_DOCUMENT.value)
        super().mousePressEvent(event)


class NextCard(Card):
    """下一步卡片"""

    def __init__(self, parent: FinshPage) -> None:
        super().__init__(parent.tr("下一步"), parent.tr("开始使用 NapCatQQ Desktop"), parent)

    def mousePressEvent(self, event):
        """鼠标点击事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            # 项目内模块导入
            from src.ui.window.guide_window.guide_window import GuideWindow

            it(GuideWindow).close()
        super().mousePressEvent(event)
