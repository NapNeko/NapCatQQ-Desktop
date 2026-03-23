# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QBoxLayout, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import CaptionLabel, CardWidget, ComboBox, FluentIcon, TitleLabel, TransparentToolButton

from src.core.api_debug import ApiDebugBotContext
from ..shared import find_index_by_data


class ApiDebugTopBar(CardWidget):
    """页面顶部上下文栏。"""

    search_requested = Signal()
    refresh_requested = Signal()
    bot_changed = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugTopBar")
        self.title_label = TitleLabel("接口调试", self)
        self.subtitle_label = CaptionLabel("基于 Action schema 与 DebugAdapter 的真实调用调试", self)
        self.subtitle_label.setWordWrap(False)
        self.bot_combo = ComboBox(self)
        self.bot_combo.setMinimumWidth(180)
        self.search_button = TransparentToolButton(FluentIcon.SEARCH, self)
        self.search_button.setToolTip("Ctrl+K 搜索接口")
        self.refresh_button = TransparentToolButton(FluentIcon.UPDATE, self)
        self.refresh_button.setToolTip("刷新接口列表")

        label_layout = QVBoxLayout()
        label_layout.setContentsMargins(0, 0, 0, 0)
        label_layout.setSpacing(2)
        label_layout.addWidget(self.title_label)
        label_layout.addWidget(self.subtitle_label)

        tools_layout = QHBoxLayout()
        tools_layout.setContentsMargins(0, 0, 0, 0)
        tools_layout.setSpacing(8)
        tools_layout.addWidget(self.bot_combo)
        tools_layout.addWidget(self.search_button)
        tools_layout.addWidget(self.refresh_button)

        self.header_layout = QBoxLayout(QBoxLayout.Direction.LeftToRight)
        self.header_layout.setContentsMargins(20, 18, 20, 18)
        self.header_layout.setSpacing(16)
        self.header_layout.addLayout(label_layout, 1)
        self.header_layout.addLayout(tools_layout, 0)
        self.setLayout(self.header_layout)

        self.bot_combo.currentIndexChanged.connect(lambda: self.bot_changed.emit(str(self.bot_combo.currentData() or "")))
        self.search_button.clicked.connect(self.search_requested.emit)
        self.refresh_button.clicked.connect(self.refresh_requested.emit)

    def populate_bots(self, contexts: list[ApiDebugBotContext], selected_bot_id: str) -> None:
        self.bot_combo.blockSignals(True)
        self.bot_combo.clear()
        for context in contexts:
            self.bot_combo.addItem(f"{context.bot_name} ({context.bot_id})", userData=context.bot_id)
        if not contexts:
            self.bot_combo.addItem("没有可用 Bot", userData="")
        index = find_index_by_data(self.bot_combo, selected_bot_id)
        self.bot_combo.setCurrentIndex(index if index >= 0 else 0)
        self.bot_combo.blockSignals(False)

    def sync_layout(self, width: int) -> None:
        compact = width < 1040
        self.header_layout.setDirection(QBoxLayout.Direction.TopToBottom if compact else QBoxLayout.Direction.LeftToRight)
        self.bot_combo.setMinimumWidth(160 if compact else 180)
