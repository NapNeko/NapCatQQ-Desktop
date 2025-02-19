# -*- coding: utf-8 -*-
"""
NapCatQQ Desktop 的设置页面 - 分割线组件
"""

# 第三方库导入
from qfluentwidgets.common import isDarkTheme
from PySide6.QtGui import QPen, QColor, QPainter
from PySide6.QtWidgets import QWidget


class Separator(QWidget):
    """分隔符"""

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self.setFixedSize(6, 16)

    def paintEvent(self, e):
        """绘制事件"""

        # 创建画家
        painter = QPainter(self)

        # 设置抗锯齿
        pen = QPen(1)
        pen.setCosmetic(True)

        # 设置画笔颜色
        c = QColor(255, 255, 255, 21) if isDarkTheme() else QColor(0, 0, 0, 15)
        pen.setColor(c)
        painter.setPen(pen)

        # 绘制分隔线
        x = self.width() // 2
        painter.drawLine(x, 0, x, self.height())

        # 释放画家
        painter.end()
