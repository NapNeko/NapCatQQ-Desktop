# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QUrl, Signal
from PySide6.QtGui import QFont, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from qfluentwidgets.common import setFont, FluentIcon
from qfluentwidgets.components import ImageLabel, TitleLabel, PushButton, PrimaryPushButton

from src.Ui.StyleSheet import StyleSheet


class ContentViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """

        super().__init__()
        self.setObjectName("content_view")

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)
