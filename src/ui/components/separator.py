# -*- coding: utf-8 -*-
"""
NapCatQQ Desktop 设置页面 - 分割线组件
"""

# 第三方库导入
from qfluentwidgets.common import isDarkTheme
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QPainter, QPen
from PySide6.QtWidgets import QWidget


class Separator(QWidget):
    """垂直分隔线组件，用于视觉上的内容区分"""

    def __init__(self, parent: QWidget | None = None):
        """
        初始化分隔线组件

        Args:
            parent: 父组件，默认为 None
        """
        super().__init__(parent)
        self.setFixedSize(6, 16)

    def paintEvent(self, event):
        """
        重绘事件处理函数，绘制分隔线

        Args:
            event: 绘制事件对象
        """
        painter = QPainter(self)
        self._draw_separator(painter)
        painter.end()

    def _draw_separator(self, painter: QPainter):
        """
        绘制分隔线的内部实现方法

        Args:
            painter: 画家对象，用于执行绘制操作
        """
        # 配置抗锯齿和画笔
        pen = QPen()
        pen.setWidth(1)
        pen.setCosmetic(True)

        # 根据主题设置颜色
        if isDarkTheme():
            color = QColor(255, 255, 255, 21)  # 深色主题下的半透明白色
        else:
            color = QColor(0, 0, 0, 15)  # 浅色主题下的半透明黑色

        pen.setColor(color)
        painter.setPen(pen)

        # 在组件中心绘制垂直线
        center_x = self.width() // 2
        painter.drawLine(center_x, 0, center_x, self.height())
