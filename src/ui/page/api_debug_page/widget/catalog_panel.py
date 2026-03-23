# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtWidgets import QListWidgetItem, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    CardWidget,
    ComboBox,
    FluentIcon,
    ListWidget,
    StrongBodyLabel,
    ToolTipFilter,
    ToolButton,
)

from src.core.api_debug import ApiDebugActionSchema
from src.ui.components.stacked_widget import TransparentStackedWidget
from ..shared import find_index_by_data


class ActionCatalogPanel(CardWidget):
    """左侧 Action 接口目录。"""

    action_selected = Signal(object)
    search_requested = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.schemas: list[ApiDebugActionSchema] = []
        self._refreshing = False

        self.title_label = StrongBodyLabel("接口目录", self)
        self.tag_combo = ComboBox(self)
        self.tag_combo.addItem("全部标签", userData="")
        self.search_button = ToolButton(FluentIcon.SEARCH, self)
        self.list_widget = ListWidget(self)
        self.list_widget.setWordWrap(True)
        self.list_widget.setTextElideMode(Qt.TextElideMode.ElideRight)
        self.list_widget.setUniformItemSizes(True)
        self.content_stack = TransparentStackedWidget(self)
        self.empty_page = QWidget(self)
        self.empty_label = CaptionLabel("当前没有可展示的接口", self.empty_page)
        self.empty_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.footer_label = CaptionLabel("", self)

        empty_layout = QVBoxLayout(self.empty_page)
        empty_layout.setContentsMargins(12, 24, 12, 24)
        empty_layout.setSpacing(6)
        empty_layout.addStretch(1)
        empty_layout.addWidget(self.empty_label, 0, Qt.AlignmentFlag.AlignHCenter)
        empty_layout.addStretch(1)

        self.content_stack.addWidget(self.list_widget)
        self.content_stack.addWidget(self.empty_page)

        filter_row = QWidget(self)
        filter_layout = QHBoxLayout(filter_row)
        filter_layout.setContentsMargins(0, 0, 0, 0)
        filter_layout.setSpacing(8)
        filter_layout.addWidget(self.tag_combo, 1)
        filter_layout.addWidget(self.search_button)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(10)
        layout.addWidget(self.title_label)
        layout.addWidget(filter_row)
        layout.addWidget(self.content_stack, 1)
        layout.addWidget(self.footer_label)

        self.search_button.clicked.connect(self.search_requested.emit)
        self.tag_combo.currentIndexChanged.connect(self._refresh_list)
        self.list_widget.currentItemChanged.connect(self._emit_current_action)
        self.footer_label.hide()
        self._setup_tooltips()

    def set_schemas(self, schemas: list[ApiDebugActionSchema], selected_action: str = "") -> None:
        self.schemas = schemas
        self._rebuild_tags()
        self._refresh_list(selected_action=selected_action)

    def set_selected_action(self, action_name: str) -> None:
        for index in range(self.list_widget.count()):
            item = self.list_widget.item(index)
            if str(item.data(Qt.ItemDataRole.UserRole) or "") == action_name:
                self.list_widget.setCurrentItem(item)
                return
        if str(self.tag_combo.currentData() or ""):
            self.tag_combo.blockSignals(True)
            self.tag_combo.setCurrentIndex(0)
            self.tag_combo.blockSignals(False)
            self._refresh_list(selected_action=action_name)

    def _rebuild_tags(self) -> None:
        previous_tag = str(self.tag_combo.currentData() or "")
        tags = sorted({tag for schema in self.schemas for tag in schema.action_tags if tag})
        self.tag_combo.blockSignals(True)
        self.tag_combo.clear()
        self.tag_combo.addItem("全部标签", userData="")
        for tag in tags:
            self.tag_combo.addItem(tag, userData=tag)
        index = find_index_by_data(self.tag_combo, previous_tag)
        self.tag_combo.setCurrentIndex(index if index >= 0 else 0)
        self.tag_combo.blockSignals(False)

    def _refresh_list(self, *_args, selected_action: str = "") -> None:
        self._refreshing = True
        previous_action = selected_action or self._current_action_name()
        selected_tag = str(self.tag_combo.currentData() or "")
        filtered: list[ApiDebugActionSchema] = []

        for schema in self.schemas:
            if selected_tag and selected_tag not in schema.action_tags:
                continue
            filtered.append(schema)

        self.list_widget.clear()
        current_row = -1
        for index, schema in enumerate(filtered):
            summary = schema.summary.strip() or schema.description.strip() or "无说明"
            item = QListWidgetItem()
            item.setData(Qt.ItemDataRole.UserRole, schema.action)
            item.setText(f"/{schema.action}\n{summary}")
            item.setToolTip(f"{summary}")
            item.setSizeHint(QSize(0, 60))
            self.list_widget.addItem(item)
            if schema.action == previous_action:
                current_row = index

        has_items = self.list_widget.count() > 0
        self.content_stack.setCurrentWidget(self.list_widget if has_items else self.empty_page)
        self.footer_label.setVisible(has_items)
        self.footer_label.setText(f"共 {self.list_widget.count()} 个接口")

        if has_items:
            self.list_widget.setCurrentRow(current_row if current_row >= 0 else 0)

        self._refreshing = False
        if not has_items:
            self.action_selected.emit(None)

    def _emit_current_action(self, current: QListWidgetItem | None, _previous: QListWidgetItem | None) -> None:
        if self._refreshing:
            return
        if current is None:
            self.action_selected.emit(None)
            return
        action_name = str(current.data(Qt.ItemDataRole.UserRole) or "")
        schema = next((item for item in self.schemas if item.action == action_name), None)
        self.action_selected.emit(schema)

    def _current_action_name(self) -> str:
        item = self.list_widget.currentItem()
        return str(item.data(Qt.ItemDataRole.UserRole) or "") if item is not None else ""

    def _setup_tooltips(self) -> None:
        self.search_button.setToolTip("打开接口搜索对话框")
        self.search_button.setToolTipDuration(1000)
        self.search_button.installEventFilter(ToolTipFilter(self.search_button, showDelay=300))
        self.tag_combo.setToolTip("按标签筛选接口")
        self.tag_combo.setToolTipDuration(1000)
        self.tag_combo.installEventFilter(ToolTipFilter(self.tag_combo, showDelay=300))
