# -*- coding: utf-8 -*-

"""
构建标题栏
"""
from loguru import logger
from typing import TYPE_CHECKING
from qfluentwidgets.window import MSFluentTitleBar
from qframelesswindow.titlebar import MaximizeButton, MinimizeButton, CloseButton
from qfluentwidgets.common import Theme

from PySide6.QtCore import Qt, QUrl, QSize, QPoint, QRectF, QPointF
from PySide6.QtGui import QIcon, QDesktopServices, QColor, QPainter, QPaintEvent, QPen, QPainterPath
from PySide6.QtSvg import QSvgRenderer

from src.Ui.Icon import NapCatDesktopIcon

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class CustomTitleBar(MSFluentTitleBar):

    def __init__(self, parent: "MainWindow") -> None:
        """
        自定义一个标题栏控件

        :param parent: MSFluentTitleBar需要指定一个父类
        """
        super().__init__(parent)

        self.set_title()
        self.set_buttons()
        logger.success("标题栏构建完成")

    def set_title(self) -> None:
        self.setTitle("NapCat Desktop")
        self.setIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT))

    def set_buttons(self) -> None:
        """配置窗口控制按钮并初始化槽连接"""
        button_info = {
            'minBtn': (MinBtn, self.window().showMinimized),
            'maxBtn': (MaxBtn, self.__toggle_maximization),
            'closeBtn': (CloseBtn, self.window().close)
        }

        for btn_name, (btn_class, slot) in button_info.items():
            self.__replace_utton(btn_class, btn_name)
            getattr(self, btn_name).clicked.connect(slot)

        self.buttonLayout.setContentsMargins(0, 8, 10, 0)

    def __replace_utton(self, btn_class, btn_name: str) -> None:
        """替换指定的按钮"""
        old_btn = getattr(self, btn_name, None)
        if old_btn:
            self.buttonLayout.removeWidget(old_btn)
            old_btn.close()

        new_btn = btn_class(self)
        new_btn.setFixedHeight(32)
        self.buttonLayout.addWidget(new_btn)
        setattr(self, btn_name, new_btn)

    def __toggle_maximization(self) -> None:
        """切换窗口的最大化和正常状态"""
        if self.window().isMaximized():
            self.window().showNormal()
        else:
            self.window().showMaximized()


class MaxBtn(MaximizeButton):

    def __init__(self, parent) -> None:
        """
        重新绘制放大按钮
        :param parent:
        """
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        color, bgColor = self._getColors()

        # 绘制背景
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # 绘制图标
        painter.setBrush(Qt.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)

        r = self.devicePixelRatioF()
        painter.scale(1 / r, 1 / r)
        if not self._isMax:
            painter.drawRect(int(18 * r), int(11 * r), int(10 * r), int(10 * r))
        else:
            painter.drawRect(int(18 * r), int(13 * r), int(8 * r), int(8 * r))
            x0 = int(18 * r) + int(2 * r)
            y0 = 13 * r
            dw = int(2 * r)
            path = QPainterPath(QPointF(x0, y0))
            path.lineTo(x0, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw + 8 * r)
            path.lineTo(x0 + 8 * r - dw, y0 - dw + 8 * r)
            painter.drawPath(path)


class MinBtn(MinimizeButton):
    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        color, bgColor = self._getColors()

        # 绘制背景
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # 绘制图标
        painter.setBrush(Qt.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)
        painter.drawLine(18, 16, 28, 16)


class CloseBtn(CloseButton):

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        painter.setRenderHints(QPainter.Antialiasing | QPainter.SmoothPixmapTransform)
        color, bgColor = self._getColors()

        # draw background
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # draw icon
        color = color.name()
        pathNodes = self._svgDom.elementsByTagName('path')
        for i in range(pathNodes.length()):
            element = pathNodes.at(i).toElement()
            element.setAttribute('stroke', color)

        renderer = QSvgRenderer(self._svgDom.toByteArray())
        renderer.render(painter, QRectF(self.rect()))
