# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QTextEdit
from qfluentwidgets import SmoothScrollDelegate, setFont
from qfluentwidgets.components.widgets.menu import TextEditMenu

from src.Ui.StyleSheet import StyleSheet
from src.Ui.resource import resource


class UpdateLogCard(QTextEdit):
    """
    ## 用于显示更新日志
    """

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self.scrollDelegate = SmoothScrollDelegate(self)
        StyleSheet.UPDATE_LOG_CARD.apply(self)
        setFont(self)

    def contextMenuEvent(self, e):
        menu = TextEditMenu(self)
        menu.exec(e.globalPos(), ani=True)
