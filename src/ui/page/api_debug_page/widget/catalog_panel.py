# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import QObject, Property, QEasingCurve, QEvent, QPropertyAnimation, QSize, Qt, Signal
from PySide6.QtGui import QColor, QPainter
from PySide6.QtWidgets import QListWidgetItem, QHBoxLayout, QLabel, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    CardWidget,
    ComboBox,
    FluentIcon,
    ListWidget,
    StrongBodyLabel,
    ToolTipFilter,
    ToolButton,
    isDarkTheme,
)

from src.core.config import cfg
from src.core.api_debug import ApiDebugActionSchema
from src.ui.components.stacked_widget import TransparentStackedWidget
from ..shared import find_index_by_data
from qfluentwidgets.components.widgets.list_view import ListItemDelegate


class _CatalogListItemDelegate(ListItemDelegate):
    """屏蔽 qfluentwidgets 默认的选中背景和左侧指示器。"""

    @staticmethod
    def _blend_color(base: QColor, target: QColor, ratio: float) -> QColor:
        ratio = max(0.0, min(1.0, ratio))
        return QColor(
            int(base.red() * (1.0 - ratio) + target.red() * ratio),
            int(base.green() * (1.0 - ratio) + target.green() * ratio),
            int(base.blue() * (1.0 - ratio) + target.blue() * ratio),
            255,
        )

    @staticmethod
    def _theme_accent_color() -> QColor:
        color = QColor(cfg.get(cfg.themeColor))
        if not color.isValid():
            color = QColor("#009faa")
        return color

    def _drawBackground(self, painter, option, index):  # noqa: ANN001
        if index.row() not in self.selectedRows:
            return

        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        accent = self._theme_accent_color()
        fill = self._blend_color(
            accent, QColor("#0f172a") if isDarkTheme() else QColor("#ffffff"), 0.84 if not isDarkTheme() else 0.34
        )
        fill.setAlpha(255)
        painter.setBrush(fill)
        painter.setPen(Qt.PenStyle.NoPen)
        # qfluentwidgets 默认列表背景就是直接基于整行 option.rect 绘制，
        # 之前把内容区内边距错误套到了背景上，导致视觉上比默认态更窄。
        # 左侧轻微内缩 1px，避免贴边时看起来像被上层裁掉。
        rect = option.rect.adjusted(1, 0, -4, -2)
        painter.drawRoundedRect(rect, 6, 6)
        painter.restore()

    def _drawIndicator(self, painter, option, index):  # noqa: ANN001
        _ = painter, option, index
        return

    def paint(self, painter, option, index):  # noqa: ANN001
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setClipping(True)
        painter.setClipRect(option.rect)
        option.rect.adjust(0, self.margin, 0, -self.margin)
        self._drawBackground(painter, option, index)
        painter.restore()


class _ActionIndicator(QWidget):
    """左侧选中指示条。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._progress = 0.0
        self._selected = False
        self._hovered = False

        self.setFixedSize(8, 34)
        self._animation = QPropertyAnimation(self, b"indicatorProgress", self)
        self._animation.setDuration(120)
        self._animation.setEasingCurve(QEasingCurve.OutQuad)

    def set_state(self, *, selected: bool, hovered: bool) -> None:
        self._selected = selected
        self._hovered = hovered
        target = 1.0 if selected else 0.16 if hovered else 0.0
        if abs(target - self._progress) < 0.001:
            return

        self._animation.stop()
        self._animation.setStartValue(self._progress)
        self._animation.setEndValue(target)
        self._animation.start()

    def _accent_color(self) -> QColor:
        color = QColor(cfg.get(cfg.themeColor))
        if not color.isValid():
            color = QColor("#009faa")
        return color

    def paintEvent(self, _event) -> None:
        if self._progress <= 0.001:
            return

        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)

        color = self._accent_color()
        color.setAlpha(56 + int(199 * self._progress))

        bar_width = 3
        bar_height = 8 + int(16 * self._progress)
        x = (self.width() - bar_width) / 2
        y = (self.height() - bar_height) / 2
        radius = bar_width / 2

        if self._selected:
            glow = QColor(color)
            glow.setAlpha(20 if isDarkTheme() else 28)
            painter.setBrush(glow)
            painter.drawRoundedRect(x - 1.5, y - 1.5, bar_width + 3, bar_height + 3, radius + 1.5, radius + 1.5)

        painter.setBrush(color)
        painter.drawRoundedRect(x, y, bar_width, bar_height, radius, radius)

    def get_indicator_progress(self) -> float:
        return self._progress

    def set_indicator_progress(self, value: float) -> None:
        self._progress = float(value)
        self.update()

    indicatorProgress = Property(float, get_indicator_progress, set_indicator_progress)


class ActionCatalogItemWidget(QWidget):
    """接口目录条目。"""

    def __init__(self, action: str, summary: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugActionItem")
        self.setAttribute(Qt.WidgetAttribute.WA_Hover, True)
        self.setFixedHeight(60)

        self.accent_bar = _ActionIndicator(self)

        self.title_label = QLabel(f"/{action}", self)
        self.title_label.setObjectName("ApiDebugActionItemTitle")

        self.summary_label = QLabel(summary, self)
        self.summary_label.setObjectName("ApiDebugActionItemSummary")

        content_layout = QVBoxLayout()
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(4)
        content_layout.addWidget(self.title_label)
        content_layout.addWidget(self.summary_label)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 10, 12, 10)
        layout.setSpacing(4)
        layout.addWidget(self.accent_bar)
        layout.addLayout(content_layout, 1)

        self._selected = False
        self._hovered = False
        self._apply_state()

    def set_selected(self, selected: bool) -> None:
        self._selected = selected
        self._apply_state()

    def set_hovered(self, hovered: bool) -> None:
        self._hovered = hovered
        self._apply_state()

    def _apply_state(self) -> None:
        self.setProperty("selected", self._selected)
        self.setProperty("hovered", self._hovered)
        self.accent_bar.set_state(selected=self._selected, hovered=self._hovered)
        self.update()

    def paintEvent(self, _event) -> None:
        super().paintEvent(_event)


class ActionCatalogPanel(CardWidget):
    """左侧 Action 接口目录。"""

    action_selected = Signal(object)
    search_requested = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.schemas: list[ApiDebugActionSchema] = []
        self._refreshing = False
        self._hovered_row = -1
        self._selected_row = -1

        self.title_label = StrongBodyLabel("接口目录", self)
        self.tag_combo = ComboBox(self)
        self.tag_combo.addItem("全部标签", userData="")
        self.search_button = ToolButton(FluentIcon.SEARCH, self)
        self.list_widget = ListWidget(self)
        self.list_widget.setWordWrap(True)
        self.list_widget.setTextElideMode(Qt.TextElideMode.ElideRight)
        self.list_widget.setUniformItemSizes(True)
        self.list_widget.setItemDelegate(_CatalogListItemDelegate(self.list_widget))
        self.list_widget.viewport().installEventFilter(self)
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
            item.setToolTip(f"{summary}")
            item.setSizeHint(QSize(0, 60))
            self.list_widget.addItem(item)
            self.list_widget.setItemWidget(item, ActionCatalogItemWidget(schema.action, summary, self.list_widget))
            if schema.action == previous_action:
                current_row = index

        has_items = self.list_widget.count() > 0
        self.content_stack.setCurrentWidget(self.list_widget if has_items else self.empty_page)
        self.footer_label.setVisible(has_items)
        self.footer_label.setText(f"共 {self.list_widget.count()} 个接口")

        if has_items:
            self.list_widget.setCurrentRow(current_row if current_row >= 0 else 0)
            self._selected_row = self.list_widget.currentRow()
            self._sync_item_visual_state(full_refresh=True)

        self._refreshing = False
        if not has_items:
            self._hovered_row = -1
            self._selected_row = -1
            self.action_selected.emit(None)

    def _emit_current_action(self, current: QListWidgetItem | None, _previous: QListWidgetItem | None) -> None:
        if self._refreshing:
            return
        if current is None:
            self.action_selected.emit(None)
            return
        previous_row = self._selected_row
        self._selected_row = self.list_widget.currentRow()
        self._sync_item_visual_state(rows={previous_row, self._selected_row})
        action_name = str(current.data(Qt.ItemDataRole.UserRole) or "")
        schema = next((item for item in self.schemas if item.action == action_name), None)
        self.action_selected.emit(schema)

    def _current_action_name(self) -> str:
        item = self.list_widget.currentItem()
        return str(item.data(Qt.ItemDataRole.UserRole) or "") if item is not None else ""

    def _sync_item_visual_state(self, *, rows: set[int] | None = None, full_refresh: bool = False) -> None:
        if full_refresh or rows is None:
            rows_to_update = range(self.list_widget.count())
        else:
            rows_to_update = [row for row in rows if row is not None and row >= 0]

        for index in rows_to_update:
            item = self.list_widget.item(index)
            if item is None:
                continue
            widget = self.list_widget.itemWidget(item)
            if isinstance(widget, ActionCatalogItemWidget):
                widget.set_selected(index == self._selected_row)
                widget.set_hovered(index == self._hovered_row)

    def eventFilter(self, watched: QObject, event: QEvent) -> bool:
        if not hasattr(self, "list_widget"):
            return super().eventFilter(watched, event)

        if watched is self.list_widget.viewport():
            if event.type() == QEvent.Type.MouseMove:
                row = self.list_widget.indexAt(event.pos()).row()
                if row != self._hovered_row:
                    previous_row = self._hovered_row
                    self._hovered_row = row
                    self._sync_item_visual_state(rows={previous_row, self._hovered_row})
            elif event.type() in {QEvent.Type.Leave, QEvent.Type.HoverLeave}:
                if self._hovered_row != -1:
                    previous_row = self._hovered_row
                    self._hovered_row = -1
                    self._sync_item_visual_state(rows={previous_row})

        return super().eventFilter(watched, event)

    def _setup_tooltips(self) -> None:
        self.search_button.setToolTip("打开接口搜索对话框")
        self.search_button.setToolTipDuration(1000)
        self.search_button.installEventFilter(ToolTipFilter(self.search_button, showDelay=300))
        self.tag_combo.setToolTip("按标签筛选接口")
        self.tag_combo.setToolTipDuration(1000)
        self.tag_combo.installEventFilter(ToolTipFilter(self.tag_combo, showDelay=300))
