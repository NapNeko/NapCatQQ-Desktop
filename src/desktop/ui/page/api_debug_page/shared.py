# -*- coding: utf-8 -*-
from __future__ import annotations

"""API 调试页仍会跨模块复用的小型共享控件。"""

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget
from qfluentwidgets import PillPushButton


class ApiDebugChip(PillPushButton):
    """顶部摘要胶囊。"""

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("ApiDebugChip")
        self.label = self
        self.setCheckable(False)
        self.setFixedHeight(28)
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.setText(text)

    def set_text(self, text: str) -> None:
        self.setText(text)
