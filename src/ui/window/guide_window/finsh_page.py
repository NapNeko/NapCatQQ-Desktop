# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import IconWidget, ImageLabel, CaptionLabel, SubtitleLabel
from qframelesswindow import FramelessWindow
from PySide6.QtGui import QColor, QPainter, QPainterPath, QDesktopServices
from PySide6.QtCore import Qt, Property, QEasingCurve, QPropertyAnimation
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.ui.components.separator import Separator
from src.core.network.urls import Urls


class FinshPage(QWidget):

    def __init__(self, parent: FramelessWindow):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = SubtitleLabel(self.tr("安装完成, 下一步?"), self)
        self.fufuLabel = ImageLabel(":/FuFuFace/image/FuFuFace/g_fufu_13.gif", self)
        self.separator = Separator(self)
        self.documentCard = DocumentCard(self)
        self.nextCard = NextCard(self)

        # 设置控件属性
        self.fufuLabel.scaledToHeight(128)

        # 设置布局
        self.cardLayout = QVBoxLayout()
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(16)
        self.cardLayout.addWidget(self.documentCard)
        self.cardLayout.addWidget(self.nextCard)

        self.hBoxLayout = QHBoxLayout()
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.hBoxLayout.setSpacing(16)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.fufuLabel)
        self.hBoxLayout.addWidget(self.separator)
        self.hBoxLayout.addLayout(self.cardLayout)
        self.hBoxLayout.addStretch(1)

        self.vBoxLayout = QVBoxLayout(self)
        self.vBoxLayout.setContentsMargins(16, 16, 16, 16)
        self.vBoxLayout.setSpacing(16)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.addWidget(self.titleLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addSpacing(32)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addStretch(1)

        # 设置属性
        self.separator.setFixedHeight(128)


class Card(QWidget):
    """卡片基类，带有悬停动画效果"""

    def __init__(self, title: str, subTitle: str, parent=None) -> None:
        super().__init__(parent)

        # 动画属性
        self._radius = 5
        self._scale = 1.0
        self._bg_color = QColor(240, 240, 240)

        # 创建控件
        self.titleLabel = BodyLabel(title, self)
        self.subTitleLabel = CaptionLabel(subTitle, self)
        self.iconLabel = IconWidget(FI.RIGHT_ARROW, self)

        # 设置属性
        self.setFixedSize(260, 60)
        self.iconLabel.setFixedSize(12, 12)

        # 设置鼠标跟踪
        self.setMouseTracking(True)

        # 布局
        self.labelLayout = QVBoxLayout()
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.setSpacing(8)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addWidget(self.subTitleLabel)

        self.hBoxLayout = QHBoxLayout(self)
        self.hBoxLayout.setContentsMargins(12, 12, 12, 12)
        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addWidget(self.iconLabel, alignment=Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)

        # 初始化动画
        self.initAnimations()

    def initAnimations(self):
        # 圆角动画
        self.radius_anim = QPropertyAnimation(self, b"radius")
        self.radius_anim.setDuration(300)
        self.radius_anim.setEasingCurve(QEasingCurve.OutQuad)

        # 缩放动画
        self.scale_anim = QPropertyAnimation(self, b"scale")
        self.scale_anim.setDuration(300)
        self.scale_anim.setEasingCurve(QEasingCurve.OutQuad)

        # 颜色动画
        self.color_anim = QPropertyAnimation(self, b"bg_color")
        self.color_anim.setDuration(300)
        self.color_anim.setEasingCurve(QEasingCurve.OutQuad)

    def enterEvent(self, event):
        # 鼠标进入时启动动画
        self.radius_anim.setStartValue(self._radius)
        self.radius_anim.setEndValue(10)

        self.scale_anim.setStartValue(self._scale)
        self.scale_anim.setEndValue(1.02)

        self.color_anim.setStartValue(self._bg_color)
        self.color_anim.setEndValue(QColor(220, 220, 220))

        self.radius_anim.start()
        self.scale_anim.start()
        self.color_anim.start()

        super().enterEvent(event)

    def leaveEvent(self, event):
        # 鼠标离开时恢复动画
        self.radius_anim.setStartValue(self._radius)
        self.radius_anim.setEndValue(5)

        self.scale_anim.setStartValue(self._scale)
        self.scale_anim.setEndValue(1.0)

        self.color_anim.setStartValue(self._bg_color)
        self.color_anim.setEndValue(QColor(240, 240, 240))

        self.radius_anim.start()
        self.scale_anim.start()
        self.color_anim.start()

        super().leaveEvent(event)

    def paintEvent(self, event):
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

        painter.fillPath(path, self._bg_color)

    # 自定义属性，用于动画
    def getRadius(self):
        return self._radius

    def setRadius(self, radius):
        self._radius = radius
        self.update()

    def getScale(self):
        return self._scale

    def setScale(self, scale):
        self._scale = scale
        self.update()

    def getBgColor(self):
        return self._bg_color

    def setBgColor(self, color):
        self._bg_color = color
        self.update()

    # 定义属性
    radius = Property(int, getRadius, setRadius)
    scale = Property(float, getScale, setScale)
    bg_color = Property(QColor, getBgColor, setBgColor)


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

            GuideWindow().close()
        super().mousePressEvent(event)
