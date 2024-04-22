# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap, QPainter
from qfluentwidgets.common import isDarkTheme
from qfluentwidgets.components import ScrollArea

from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class PageBase(ScrollArea):
    """
    页面基类
    """

    def __init__(self):
        super().__init__()
        # 定义背景图片原图 QPixmap
        self._bg_pixmap_light = QPixmap(":Global/image/Global/page_bg_light.png")
        self._bg_pixmap_dark = QPixmap(":Global/image/Global/page_bg_dark.png")

    def updateBgImage(self) -> None:
        """
        用于更新图片大小
        """
        # 重新加载图片保证缩放后清晰
        if not isDarkTheme():
            self.bg_pixmap = self._bg_pixmap_light
        else:
            self.bg_pixmap = self._bg_pixmap_dark

        self.bg_pixmap = self.bg_pixmap.scaled(
            self.size(),
            aspectMode=Qt.AspectRatioMode.KeepAspectRatioByExpanding,  # 等比缩放
            mode=Qt.TransformationMode.SmoothTransformation  # 平滑效果
        )
        self.update()

    def paintEvent(self, event) -> None:
        """
        重写绘制事件绘制背景图片
        """
        painter = QPainter(self.viewport())
        painter.drawPixmap(self.rect(), self.bg_pixmap)
        super().paintEvent(event)

    def resizeEvent(self, event) -> None:
        """
        重写缩放事件
        """
        self.updateBgImage()
        super().resizeEvent(event)
