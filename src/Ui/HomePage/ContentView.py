# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QWidget

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
