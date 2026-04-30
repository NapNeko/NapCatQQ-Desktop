# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QKeyEvent, QMouseEvent
from PySide6.QtWidgets import QHBoxLayout, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    CardWidget,
    SimpleCardWidget,
    ComboBox,
    FluentIcon,
    SearchLineEdit,
    StrongBodyLabel,
    ToolTipFilter,
    ToolButton,
)

from src.core.api_debug import ApiDebugActionSchema, ApiDebugBotContext
from src.ui.components.stacked_widget import TransparentStackedWidget
from ..common import find_index_by_data


class _DialogSearchLineEdit(SearchLineEdit):
    """只负责拉起搜索对话框的只读搜索框。"""

    activated = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setReadOnly(True)
        self.setPlaceholderText("点击搜索接口")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self.activated.emit()
        super().mousePressEvent(event)

    def keyPressEvent(self, event: QKeyEvent) -> None:
        if event.key() in {Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Space}:
            self.activated.emit()
            event.accept()
            return
        super().keyPressEvent(event)


class ActionCatalogPanel(SimpleCardWidget):
    """左侧 Action 接口目录。"""

    action_selected = Signal(object)
    search_requested = Signal()
    refresh_requested = Signal()
    bot_changed = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.schemas: list[ApiDebugActionSchema] = []
        self._schema_by_action: dict[str, ApiDebugActionSchema] = {}
        self._action_items: dict[str, QTreeWidgetItem] = {}
        self._refreshing = False

        self.title_label = StrongBodyLabel("接口目录", self)
        self.bot_combo = ComboBox(self)
        self.bot_combo.setMinimumWidth(180)
        self.refresh_button = ToolButton(FluentIcon.UPDATE, self)
        self.search_edit = _DialogSearchLineEdit(self)
        self.tree_widget = QTreeWidget(self)
        self.tree_widget.setHeaderHidden(True)
        self.tree_widget.setUniformRowHeights(True)
        self.tree_widget.setIndentation(18)
        self.tree_widget.setRootIsDecorated(True)
        self.tree_widget.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.tree_widget.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
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

        self.content_stack.addWidget(self.tree_widget)
        self.content_stack.addWidget(self.empty_page)

        header_row = QWidget(self)
        header_layout = QHBoxLayout(header_row)
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)
        header_layout.addWidget(self.title_label)
        header_layout.addStretch(1)
        header_layout.addWidget(self.refresh_button)

        bot_row = QWidget(self)
        bot_layout = QHBoxLayout(bot_row)
        bot_layout.setContentsMargins(0, 0, 0, 0)
        bot_layout.setSpacing(8)
        bot_layout.addWidget(self.bot_combo, 1)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(10)
        layout.addWidget(header_row)
        layout.addWidget(bot_row)
        layout.addWidget(self.search_edit)
        layout.addWidget(self.content_stack, 1)
        layout.addWidget(self.footer_label)

        self.search_edit.activated.connect(self.search_requested.emit)
        self.refresh_button.clicked.connect(self.refresh_requested.emit)
        self.bot_combo.currentIndexChanged.connect(
            lambda: self.bot_changed.emit(str(self.bot_combo.currentData() or ""))
        )
        self.tree_widget.currentItemChanged.connect(self._emit_current_action)
        self.tree_widget.itemClicked.connect(self._handle_item_clicked)
        self.footer_label.hide()
        self._setup_tooltips()

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

    def set_schemas(self, schemas: list[ApiDebugActionSchema], selected_action: str = "") -> None:
        self.schemas = schemas
        self._schema_by_action = {schema.action: schema for schema in schemas}
        self._rebuild_tree(selected_action=selected_action)

    def set_selected_action(self, action_name: str, *, expand_parent: bool = True) -> None:
        item = self._action_items.get(action_name)
        if item is None:
            return
        parent_item = item.parent()
        self.tree_widget.setCurrentItem(item)
        if parent_item is not None:
            parent_item.setExpanded(expand_parent)

    @staticmethod
    def category_name(schema: ApiDebugActionSchema) -> str:
        return next((tag.strip() for tag in schema.action_tags if tag and tag.strip()), "未分类")

    @staticmethod
    def display_name(schema: ApiDebugActionSchema) -> str:
        return schema.summary.strip() or schema.description.strip() or schema.action

    def _rebuild_tree(self, *, selected_action: str = "") -> None:
        self._refreshing = True
        previous_action = selected_action or self._current_action_name()
        self._action_items.clear()
        self.tree_widget.clear()

        grouped: dict[str, list[ApiDebugActionSchema]] = {}
        for schema in self.schemas:
            grouped.setdefault(self.category_name(schema), []).append(schema)

        for category_name in sorted(grouped):
            parent_item = QTreeWidgetItem([category_name])
            parent_item.setData(0, Qt.ItemDataRole.UserRole, "")
            self.tree_widget.addTopLevelItem(parent_item)
            for schema in sorted(grouped[category_name], key=lambda item: (self.display_name(item), item.action)):
                child_item = QTreeWidgetItem([self.display_name(schema)])
                child_item.setData(0, Qt.ItemDataRole.UserRole, schema.action)
                parent_item.addChild(child_item)
                self._action_items[schema.action] = child_item
            parent_item.setExpanded(False)

        has_items = bool(self._action_items)
        self.content_stack.setCurrentWidget(self.tree_widget if has_items else self.empty_page)
        self.footer_label.hide()

        if has_items:
            target_item = self._action_items.get(previous_action)
            if target_item is None:
                first_category = self.tree_widget.topLevelItem(0)
                target_item = (
                    first_category.child(0) if first_category is not None and first_category.childCount() else None
                )
            if target_item is not None:
                self.tree_widget.setCurrentItem(target_item)
                parent_item = target_item.parent()
                if parent_item is not None:
                    parent_item.setExpanded(False)
        else:
            self.action_selected.emit(None)

        self._refreshing = False

    def _emit_current_action(self, current: QTreeWidgetItem | None, _previous: QTreeWidgetItem | None) -> None:
        if self._refreshing or current is None:
            return
        action_name = str(current.data(0, Qt.ItemDataRole.UserRole) or "")
        if not action_name:
            return
        self.action_selected.emit(self._schema_by_action.get(action_name))

    def _current_action_name(self) -> str:
        item = self.tree_widget.currentItem()
        return str(item.data(0, Qt.ItemDataRole.UserRole) or "") if item is not None else ""

    def _handle_item_clicked(self, item: QTreeWidgetItem, _column: int) -> None:
        if item.parent() is None:
            item.setExpanded(not item.isExpanded())

    def _setup_tooltips(self) -> None:
        self.search_edit.setToolTip("打开接口搜索对话框")
        self.search_edit.setToolTipDuration(1000)
        self.search_edit.installEventFilter(ToolTipFilter(self.search_edit, showDelay=300))

        self.bot_combo.setToolTip("选择用于查看接口文档的 Bot")
        self.bot_combo.setToolTipDuration(1000)
        self.bot_combo.installEventFilter(ToolTipFilter(self.bot_combo, showDelay=300))

        self.refresh_button.setToolTip("刷新接口列表")
        self.refresh_button.setToolTipDuration(1000)
        self.refresh_button.installEventFilter(ToolTipFilter(self.refresh_button, showDelay=300))
