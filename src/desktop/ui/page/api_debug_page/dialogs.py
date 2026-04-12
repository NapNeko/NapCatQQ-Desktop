# -*- coding: utf-8 -*-
from __future__ import annotations

"""API 调试页对话框。"""

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QListWidgetItem, QWidget
from qfluentwidgets import ListWidget, MessageBoxBase, SearchLineEdit, TitleLabel

from src.desktop.core.api_debug.models import ApiDebugSearchItem


class ApiDebugSearchDialog(MessageBoxBase):
    """Action 搜索对话框。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.items: list[ApiDebugSearchItem] = []
        self.selected_item: ApiDebugSearchItem | None = None

        self.title_label = TitleLabel("搜索接口", self)
        self.search_edit = SearchLineEdit(self)
        self.search_edit.setPlaceholderText("搜索 Action 名称或简要说明")
        self.result_list = ListWidget(self)
        self.result_list.setUniformItemSizes(False)
        self.widget.setMinimumSize(620, 520)
        self.yesButton.setText("定位接口")
        self.cancelButton.setText("关闭")
        self.yesButton.setEnabled(False)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.search_edit)
        self.viewLayout.addWidget(self.result_list, 1)

        self.search_edit.textChanged.connect(self._rebuild_list)
        self.result_list.itemActivated.connect(self._accept_item)
        self.result_list.itemDoubleClicked.connect(self._accept_item)
        self.result_list.currentItemChanged.connect(self._handle_current_item_changed)
        self.yesButton.clicked.connect(self._accept_current_item)

    def set_items(self, items: list[ApiDebugSearchItem]) -> None:
        self.items = items
        self._rebuild_list()

    def open_and_choose(self, keyword: str = "") -> ApiDebugSearchItem | None:
        self.selected_item = None
        self.search_edit.setText(keyword)
        self.search_edit.selectAll()
        self.search_edit.setFocus()
        if self.exec():
            return self.selected_item
        return None

    def _rebuild_list(self) -> None:
        keyword = self.search_edit.text().strip().lower()
        self.result_list.clear()
        filtered = [
            item
            for item in self.items
            if not keyword
            or keyword in item.title.lower()
            or keyword in item.subtitle.lower()
            or keyword in item.mode.value
        ]

        for item in filtered[:200]:
            widget_item = QListWidgetItem(f"{item.title}\n{item.subtitle}")
            widget_item.setData(Qt.ItemDataRole.UserRole, item)
            self.result_list.addItem(widget_item)

        if self.result_list.count():
            self.result_list.setCurrentRow(0)
        else:
            self.selected_item = None
            self.yesButton.setEnabled(False)

    def _accept_item(self, item: QListWidgetItem) -> None:
        payload = item.data(Qt.ItemDataRole.UserRole)
        if isinstance(payload, ApiDebugSearchItem):
            self.selected_item = payload
            self.accept()

    def _handle_current_item_changed(self, current: QListWidgetItem | None, _previous: QListWidgetItem | None) -> None:
        payload = current.data(Qt.ItemDataRole.UserRole) if current is not None else None
        self.selected_item = payload if isinstance(payload, ApiDebugSearchItem) else None
        self.yesButton.setEnabled(self.selected_item is not None)

    def _accept_current_item(self) -> None:
        if self.selected_item is not None:
            self.accept()
