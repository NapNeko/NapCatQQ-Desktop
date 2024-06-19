# -*- coding: utf-8 -*-
from pathlib import Path

from PySide6.QtCore import QUrl, QRectF, Qt, Slot, Signal
from PySide6.QtGui import QPainter, QColor, QBrush
from PySide6.QtWidgets import QWidget, QVBoxLayout, QProgressBar, QHBoxLayout

from qfluentwidgets import PrimaryPushButton, FluentIcon, BodyLabel, ProgressRing, isDarkTheme, \
    IndeterminateProgressRing

from src.Core.NetworkFunc import Downloader


class ProgressBarButton(PrimaryPushButton):
    """
    ## 带下载进度条的按钮
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        # 创建属性
        self._indeterminate = False
        self._testVisible = True

        # 创建控件
        self.setText(self.tr("Install"))
        self._progressRing = ProgressRing(self)
        self._indeterminateProgressRing = IndeterminateProgressRing(self)
        self._viewLayout = QHBoxLayout(self)

        # 设置控件
        self.setFixedSize(140, 34)
        self._progressRing.setCustomBackgroundColor(QColor(254, 254, 254, 97), QColor(255, 255, 255, 34))
        self._progressRing.setCustomBarColor(QColor(255, 255, 255), QColor(255, 255, 255, 34))
        self._progressRing.setStrokeWidth(2)
        self._progressRing.setFixedSize(self.iconSize().width() + 6, self.iconSize().height() + 6)
        self._indeterminateProgressRing.setCustomBackgroundColor(QColor(254, 254, 254, 97), QColor(255, 255, 255, 34))
        self._indeterminateProgressRing.setCustomBarColor(QColor(255, 255, 255), QColor(255, 255, 255, 34))
        self._indeterminateProgressRing.setStrokeWidth(2)
        self._indeterminateProgressRing.setFixedSize(self.iconSize().width() + 6, self.iconSize().height() + 6)

        self._viewLayout.addWidget(self._progressRing, 0, Qt.AlignmentFlag.AlignCenter)
        self._viewLayout.addWidget(self._indeterminateProgressRing, 0, Qt.AlignmentFlag.AlignCenter)
        self._viewLayout.setContentsMargins(0, 0, 0, 0)

        # 调用方法
        self.setTestVisible(True)

    def setValue(self, value: int):
        """
        ## 设置 _progressRing 的进度
        """
        self._progressRing.setValue(value)

    def setTestVisible(self, value: bool):
        """
        ## 设置按钮内容是否可见
        """
        self._testVisible = value
        if self._testVisible:
            self._indeterminateProgressRing.hide()
            self._progressRing.hide()
            self.setText(self.tr("Install"))
        else:
            self.setProgressBarState(self._indeterminate)
            self.setText("")

    def setProgressBarState(self, value: bool):
        """
        ## 设置控件显示哪个进度条
        """
        self._indeterminate = value
        if self._indeterminate:
            self._indeterminateProgressRing.show()
            self._progressRing.hide()
        else:
            self._indeterminateProgressRing.hide()
            self._progressRing.show()

    def paintEvent(self, e):
        """
        ## 重写事件绘制图标
        """
        super().paintEvent(e)

        if self._testVisible:
            return

        painter = QPainter(self)
        painter.setRenderHints(
            QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform
        )
        brush = QBrush(QColor(255, 255, 255))
        painter.setBrush(brush)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(QRectF((self.width() - 8) / 2, (self.height() - 8) / 2, 8, 8), 2, 2)
        painter.end()
