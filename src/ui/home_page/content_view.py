# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QUrl, Signal
from PySide6.QtGui import QFont, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from qfluentwidgets.common import setFont, FluentIcon
from qfluentwidgets.components import ImageLabel, TitleLabel, PushButton, PrimaryPushButton

from src.ui.style_sheet import StyleSheet


class ContentViewWidget(QWidget):

    def __init__(self):
        """
        初始化
        """

        super().__init__()
        self.setObjectName("content_view")

        #
