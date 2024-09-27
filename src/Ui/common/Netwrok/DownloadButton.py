# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import ProgressRing, PrimaryPushButton, IndeterminateProgressRing
from PySide6.QtGui import QBrush, QColor, QPainter
from PySide6.QtCore import Qt, QRectF
from PySide6.QtWidgets import QHBoxLayout


class ProgressBarButton(PrimaryPushButton):
    """
    ## 带下载进度条的按钮
    """

    def __init__(self, text, parent=None) -> None:
        super().__init__(parent)
        # 创建属性
        self._text = text
        self._indeterminate = False
        self._testVisible = True

        # 创建控件
        self.setText(self._text)
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

    def setValue(self, value: int) -> None:
        """
        ## 设置 _progressRing 的进度
        """
        self._progressRing.setValue(value)

    def setTestVisible(self, value: bool) -> None:
        """
        ## 设置按钮内容是否可见
            - True : 显示文字隐藏进度条
            - False: 隐藏文字显示进度条
        """
        self._testVisible = value
        if self._testVisible:
            self._indeterminateProgressRing.hide()
            self._progressRing.hide()
            self.setText(self._text)
        else:
            self.setProgressBarState(self._indeterminate)
            self.setText("")

    def setProgressBarState(self, value: bool) -> None:
        """
        ## 设置控件显示哪个进度条
            - True : 未知进度模式
            - False: 进度模式

        # 无论是哪种模式, 都会关闭文字模式
        """
        self._indeterminate = value
        self.setText("")
        if self._indeterminate:
            # 如果为True, 则显示未知进度条
            self._indeterminateProgressRing.show()
            self._progressRing.hide()
        else:
            # 否则是进度模式
            self._indeterminateProgressRing.hide()
            self._progressRing.show()

    def paintEvent(self, e) -> None:
        """
        ## 重写事件绘制图标
        """
        super().paintEvent(e)

        if self._testVisible:
            return

        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)
        brush = QBrush(QColor(255, 255, 255))
        painter.setBrush(brush)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(QRectF((self.width() - 8) / 2, (self.height() - 8) / 2, 8, 8), 2, 2)
        painter.end()
