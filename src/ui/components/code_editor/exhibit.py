# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import SmoothScrollDelegate, setFont
from qfluentwidgets.components.widgets.menu import TextEditMenu
from PySide6.QtGui import QMouseEvent, QDesktopServices
from PySide6.QtCore import Qt, QUrl
from PySide6.QtWidgets import QTextBrowser

# 项目内模块导入
from src.ui.common.code_editor.editor import CodeEditor


class CodeExibit(CodeEditor):

    def __init__(self, parent=...):
        super().__init__(parent)
        self.setReadOnly(True)


class UpdateLogExhibit(QTextBrowser):
    """
    ## 更新日志页面使用的透明文本框
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self.scroll_delegate = SmoothScrollDelegate(self)
        self.setReadOnly(True)
        self.setOpenExternalLinks(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        setFont(self)

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton and self.anchorAt(event.pos()):
            QDesktopServices.openUrl(QUrl(self.anchorAt(event.pos())))
        else:
            super().mousePressEvent(event)

    def contextMenuEvent(self, event):
        menu = TextEditMenu(self)
        menu.exec(event.globalPos(), ani=True)
