# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from PySide6.QtCore import QPointF, QSize, Qt, Signal
from PySide6.QtGui import QColor
from PySide6.QtGui import QFont, QFontMetrics, QPainter, QPen, QPolygonF
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import CaptionLabel, CardWidget, StrongBodyLabel

from src.core.config import cfg
from src.ui.components.code_editor.exhibit import CodeExibit
from ..common import pretty_json


class _CardDivider(QWidget):
    """轻量分割线。"""

    def __init__(self, parent: QWidget | None = None, *, inset: int = 0) -> None:
        super().__init__(parent)
        self.setFixedHeight(2)
        self._inset = max(0, inset)

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        pen = QPen(QColor(127, 127, 127, 32))
        pen.setWidth(1)
        painter.setPen(pen)
        y = self.height() / 2
        start_x = self._inset
        end_x = max(start_x, self.width() - self._inset)
        painter.drawLine(start_x, int(y), end_x, int(y))


class _ExampleFooterBar(QWidget):
    """卡片底部一体化示例控制栏。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugExampleBar")

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(255, 255, 255, 0))
        painter.drawRoundedRect(self.rect(), 12, 12)


class _ExampleToggleBar(QWidget):
    """透明示例展开栏。"""

    clicked = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._expanded = False
        self.setObjectName("ApiDebugExampleToggle")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setMinimumHeight(42)

    def set_expanded(self, expanded: bool) -> None:
        self._expanded = expanded
        self.update()

    def text(self) -> str:
        return "示例  ▲" if self._expanded else "示例  ▼"

    def mousePressEvent(self, event) -> None:  # noqa: ANN001
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()
            event.accept()
            return
        super().mousePressEvent(event)

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        text_rect = self.rect().adjusted(14, 0, -14, 0)
        painter.setPen(QColor(24, 24, 27, 230))
        font = QFont(self.font())
        font.setWeight(QFont.Weight.Medium)
        painter.setFont(font)
        painter.drawText(text_rect, Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, self.text())


@dataclass(slots=True)
class _SchemaFieldNode:
    name: str
    type_name: str
    description: str
    required_text: str
    enum_values: list[str] = field(default_factory=list)
    children: list["_SchemaFieldNode"] = field(default_factory=list)


class _PaintedChip(QWidget):
    """自绘文本卡片。"""

    def __init__(
        self,
        text: str,
        parent: QWidget | None = None,
        *,
        text_color: QColor | None = None,
        background_color: QColor | None = None,
        border_color: QColor | None = None,
    ) -> None:
        super().__init__(parent)
        self._text = text
        self._text_color = text_color or QColor(24, 24, 27, 230)
        self._background_color = background_color or QColor(255, 255, 255, 24)
        self._border_color = border_color or QColor(255, 255, 255, 0)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)

    def sizeHint(self) -> QSize:
        metrics = QFontMetrics(self._chip_font())
        return QSize(metrics.horizontalAdvance(self._text) + 18, metrics.height() + 8)

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        pen = QPen(self._border_color)
        pen.setWidth(1)
        painter.setPen(pen)
        painter.setBrush(self._background_color)
        painter.drawRoundedRect(self.rect().adjusted(0, 0, -1, -1), 6, 6)

        painter.setPen(self._text_color)
        painter.setFont(self._chip_font())
        painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, self._text)

    @staticmethod
    def _chip_font() -> QFont:
        font = QFont()
        font.setPointSizeF(9)
        font.setWeight(QFont.Weight.DemiBold)
        return font


class _ParamNameChip(_PaintedChip):
    """字段名称自绘卡片。"""

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(text, parent)
        cfg.themeChanged.connect(self._refresh_palette)
        cfg.themeColorChanged.connect(self._refresh_palette)
        self._refresh_palette()

    def _refresh_palette(self, *_args) -> None:
        theme_color = QColor(cfg.get(cfg.themeColor))
        if not theme_color.isValid():
            theme_color = QColor("#2563eb")

        self._text_color = theme_color.darker(120)
        self._background_color = QColor(theme_color)
        self._background_color.setAlpha(26)
        self._border_color = QColor(theme_color)
        self._border_color.setAlpha(58)
        self.update()


class _EnumChip(_PaintedChip):
    """枚举值自绘卡片。"""

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(
            text,
            parent,
            text_color=QColor(63, 63, 70, 235),
            background_color=QColor(113, 113, 122, 26),
            border_color=QColor(113, 113, 122, 30),
        )


class ApiDebugParamRow(QWidget):
    """参数行。"""

    def __init__(
        self,
        name: str,
        type_name: str,
        description: str,
        required_text: str,
        parent: QWidget | None = None,
        *,
        enum_values: list[str] | None = None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugParamRow")

        self.name_label = _ParamNameChip(name, self)
        self.type_label = CaptionLabel(type_name, self)
        self.type_label.setObjectName("ApiDebugParamType")
        self.description_label = CaptionLabel(description or "-", self)
        self.description_label.setObjectName("ApiDebugParamDescription")
        self.required_label = CaptionLabel(required_text, self)
        self.required_label.setObjectName("ApiDebugParamRequired")
        self.enum_hint_label = CaptionLabel("枚举值:", self)
        self.enum_hint_label.setObjectName("ApiDebugParamDescription")
        self.enum_hint_label.hide()
        self.enum_chip_widgets: list[_EnumChip] = []

        left_layout = QHBoxLayout()
        left_layout.setContentsMargins(0, 0, 0, 0)
        left_layout.setSpacing(8)
        left_layout.addWidget(self.name_label, 0)
        left_layout.addWidget(self.type_label, 0)
        left_layout.addWidget(self.description_label, 0)
        if enum_values:
            self.enum_hint_label.show()
            left_layout.addWidget(self.enum_hint_label, 0)
            for enum_value in enum_values:
                chip = _EnumChip(enum_value, self)
                self.enum_chip_widgets.append(chip)
                left_layout.addWidget(chip, 0)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(8)
        layout.addLayout(left_layout, 0)
        layout.addStretch(1)
        layout.addWidget(self.required_label, 0)


class _TreeToggleIcon(QWidget):
    """统一尺寸的树节点箭头。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._expanded = True
        self._active = True
        self.setFixedSize(14, 14)

    def set_expanded(self, expanded: bool) -> None:
        self._expanded = expanded
        self.update()

    def set_active(self, active: bool) -> None:
        self._active = active
        self.update()

    def paintEvent(self, _event) -> None:
        if not self._active:
            return
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(24, 24, 27, 116))

        cx = self.width() / 2
        cy = self.height() / 2
        half = 3.5
        if self._expanded:
            polygon = QPolygonF(
                [
                    QPointF(cx - half, cy - 1.5),
                    QPointF(cx + half, cy - 1.5),
                    QPointF(cx, cy + half),
                ]
            )
        else:
            polygon = QPolygonF(
                [
                    QPointF(cx - 1.5, cy - half),
                    QPointF(cx - 1.5, cy + half),
                    QPointF(cx + half, cy),
                ]
            )
        painter.drawPolygon(polygon)


class _SchemaNodeWidget(QWidget):
    """树形 schema 节点。"""

    _TREE_LINE_COLOR = QColor(148, 163, 184, 72)
    _TREE_LINE_ACTIVE_COLOR = QColor(99, 102, 241, 82)
    _TREE_INDENT = 22

    def __init__(self, node: _SchemaFieldNode, parent: QWidget | None = None, *, depth: int = 0) -> None:
        super().__init__(parent)
        self._node = node
        self._depth = depth
        self._expanded = True
        self._has_children = bool(node.children)

        self.arrow_label = _TreeToggleIcon(self)
        self.arrow_label.set_active(self._has_children)
        self.arrow_label.setVisible(self._has_children)
        self.row_widget = ApiDebugParamRow(
            node.name,
            node.type_name,
            node.description,
            node.required_text,
            self,
            enum_values=node.enum_values,
        )

        self.header = QWidget(self)
        header_layout = QHBoxLayout(self.header)
        header_layout.setContentsMargins(4, 0, 0, 0)
        header_layout.setSpacing(6)
        header_layout.addWidget(self.arrow_label, 0, Qt.AlignmentFlag.AlignVCenter)
        header_layout.addWidget(self.row_widget, 1, Qt.AlignmentFlag.AlignVCenter)

        self.children_container = QWidget(self)
        self.children_layout = QVBoxLayout(self.children_container)
        self.children_layout.setContentsMargins(self._TREE_INDENT, 6, 0, 0)
        self.children_layout.setSpacing(8)
        for child in node.children:
            self.children_layout.addWidget(_SchemaNodeWidget(child, self.children_container, depth=depth + 1))

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        layout.addWidget(self.header)
        layout.addWidget(self.children_container)

        self.children_container.setVisible(self._has_children)
        if self._has_children:
            self.setCursor(Qt.CursorShape.PointingHandCursor)
            self.header.installEventFilter(self)
            self.arrow_label.installEventFilter(self)
            self.row_widget.installEventFilter(self)

    def eventFilter(self, watched, event) -> bool:  # noqa: ANN001
        if self._has_children and watched in {self.arrow_label, self.row_widget}:
            if event.type() == event.Type.MouseButtonPress and event.button() == Qt.MouseButton.LeftButton:
                self._toggle_expanded()
                event.accept()
                return True
        return super().eventFilter(watched, event)

    def _toggle_expanded(self) -> None:
        self._expanded = not self._expanded
        self.children_container.setVisible(self._expanded)
        self.arrow_label.set_expanded(self._expanded)

    def paintEvent(self, event) -> None:  # noqa: ANN001
        super().paintEvent(event)
        if self._depth <= 0:
            return

        anchor_x = self._branch_anchor_x()
        row_center_y = self.header.geometry().center().y()
        row_left = max(anchor_x + 6, self.row_widget.geometry().left() - 8)

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        trunk_pen = QPen(self._TREE_LINE_COLOR)
        trunk_pen.setWidth(1)
        trunk_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        trunk_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setPen(trunk_pen)

        top_y = max(0, self.header.geometry().top())
        painter.drawLine(anchor_x, top_y, anchor_x, row_center_y)
        painter.drawLine(anchor_x, row_center_y, row_left, row_center_y)

        if self._has_children and self.children_container.isVisible():
            active_pen = QPen(self._TREE_LINE_ACTIVE_COLOR)
            active_pen.setWidth(1)
            active_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            active_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            painter.setPen(active_pen)
            bottom_y = max(row_center_y, self.children_container.geometry().bottom() - 2)
            painter.drawLine(anchor_x, row_center_y, anchor_x, bottom_y)

    def _branch_anchor_x(self) -> int:
        if self.arrow_label.isVisible():
            return self.header.geometry().left() + self.arrow_label.geometry().center().x()
        return self.row_widget.geometry().left() - 14


class ApiDebugSchemaCard(CardWidget):
    """Schema 字段展示卡片。"""

    def __init__(
        self,
        title: str,
        *,
        include_example: bool = False,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugSchemaCard")
        self._include_example = include_example

        self.header_widget = QWidget(self)
        self.header_widget.setObjectName("ApiDebugSchemaContent")
        self.title_label = StrongBodyLabel(title, self.header_widget)
        self.content_type_chip = CaptionLabel("application/json", self.header_widget)
        self.content_type_chip.setObjectName("ApiDebugContentTypeChip")
        self.header_divider = _CardDivider(self)
        self.header_divider.setObjectName("ApiDebugBodyDivider")

        self.body_widget = QWidget(self)
        self.body_widget.setObjectName("ApiDebugSchemaContent")
        self.rows_container = QWidget(self.body_widget)
        self.rows_layout = QVBoxLayout(self.rows_container)
        self.rows_layout.setContentsMargins(0, 0, 0, 0)
        self.rows_layout.setSpacing(6)

        self.schema_preview = CodeExibit(self)
        self.schema_preview.setMinimumHeight(120)
        self.schema_preview.hide()

        self.example_bar: QWidget | None = None
        self.example_toggle_button: _ExampleToggleBar | None = None
        self.example_footer_divider: QWidget | None = None
        self.example_view: CodeExibit | None = None

        self._setup_layout()

    def _setup_layout(self) -> None:
        header_layout = QHBoxLayout()
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)
        header_layout.addWidget(self.title_label)
        header_layout.addWidget(self.content_type_chip, 0)
        header_layout.addStretch(1)

        header_widget_layout = QVBoxLayout(self.header_widget)
        header_widget_layout.setContentsMargins(12, 12, 12, 12)
        header_widget_layout.setSpacing(0)
        header_widget_layout.addLayout(header_layout)

        body_layout = QVBoxLayout(self.body_widget)
        body_layout.setContentsMargins(12, 12, 12, 12)
        body_layout.setSpacing(10)
        body_layout.addWidget(self.rows_container)
        body_layout.addWidget(self.schema_preview)

        card_layout = QVBoxLayout(self)
        card_layout.setContentsMargins(0, 0, 0, 0)
        card_layout.setSpacing(0)
        card_layout.addWidget(self.header_widget)
        card_layout.addWidget(self.header_divider)
        card_layout.addWidget(self.body_widget)

        if self._include_example:
            self.example_bar = _ExampleFooterBar(self)
            self.example_footer_divider = _CardDivider(self.example_bar, inset=0)
            self.example_footer_divider.setObjectName("ApiDebugBodyDivider")
            self.example_toggle_button = _ExampleToggleBar(self.example_bar)
            self.example_toggle_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            self.example_view = CodeExibit(self)
            self.example_view.setMinimumHeight(160)
            self.example_view.hide()

            example_bar_layout = QVBoxLayout(self.example_bar)
            example_bar_layout.setContentsMargins(0, 0, 0, 0)
            example_bar_layout.setSpacing(0)
            example_bar_layout.addWidget(self.example_footer_divider)
            example_bar_layout.addWidget(self.example_toggle_button)

            card_layout.addWidget(self.example_bar)
            card_layout.addWidget(self.example_view)

    def set_content_type(self, content_type: str) -> None:
        self.content_type_chip.setText(content_type or "application/json")

    def set_example_text(self, text: str) -> None:
        if self.example_view is not None:
            self.example_view.setPlainText(text or "{}")

    def set_example_visible(self, visible: bool) -> None:
        if self.example_view is None or self.example_toggle_button is None:
            return
        self.example_view.setVisible(visible)
        self.example_toggle_button.set_expanded(visible)

    def toggle_example(self) -> None:
        if self.example_view is None:
            return
        self.set_example_visible(not self.example_view.isVisible())

    def rebuild_rows(self, schema: Any, *, empty_text: str, show_schema_preview_when_needed: bool = False) -> None:
        while self.rows_layout.count():
            item = self.rows_layout.takeAt(0)
            if item is None:
                continue
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        nodes = self._extract_param_fields(schema)
        should_show_preview = bool(schema) and show_schema_preview_when_needed and not nodes
        self.schema_preview.setVisible(should_show_preview)

        if not nodes:
            self.rows_layout.addWidget(CaptionLabel(empty_text, self.rows_container))
            return

        for node in nodes:
            self.rows_layout.addWidget(_SchemaNodeWidget(node, self.rows_container))

    def set_schema_preview(self, schema: Any) -> None:
        self.schema_preview.setPlainText(pretty_json(schema or {}))

    @classmethod
    def _extract_param_fields(cls, payload_schema: Any) -> list[_SchemaFieldNode]:
        if not isinstance(payload_schema, dict):
            return []

        normalized_schema = cls._merge_all_of_schema(payload_schema)
        properties = normalized_schema.get("properties")
        if isinstance(properties, dict):
            required_fields = {
                str(field_name).strip()
                for field_name in normalized_schema.get("required", [])
                if str(field_name).strip()
            }
            return [
                cls._build_schema_node(
                    field_name=str(field_name),
                    field_schema=field_schema if isinstance(field_schema, dict) else {},
                    required_text="必填" if str(field_name) in required_fields else "可选",
                )
                for field_name, field_schema in properties.items()
            ]

        return [cls._build_schema_node(field_name="value", field_schema=normalized_schema, required_text="-")]

    @classmethod
    def _build_schema_node(
        cls,
        *,
        field_name: str,
        field_schema: dict[str, Any],
        required_text: str,
    ) -> _SchemaFieldNode:
        schema = cls._merge_all_of_schema(field_schema)
        description = cls._schema_description(schema)
        enum_values = cls._schema_enum_values(schema)

        union_variants = cls._union_variants(schema)
        if union_variants:
            union_enum_values = [
                item.get("const") for item in union_variants if isinstance(item, dict) and item.get("const") is not None
            ]
            if len(union_enum_values) == len(union_variants) and union_enum_values:
                enum_schema = dict(schema)
                enum_schema["enum"] = union_enum_values
                enum_schema.pop("oneOf", None)
                enum_schema.pop("anyOf", None)
                return _SchemaFieldNode(
                    name=field_name,
                    type_name=cls._schema_type_name(enum_schema),
                    description=description or " / ".join(str(item) for item in union_enum_values),
                    required_text=required_text,
                    enum_values=[str(item) for item in union_enum_values if item is not None],
                )

            children = cls._merge_union_children(union_variants)
            return _SchemaFieldNode(
                name=field_name,
                type_name=cls._schema_type_name(schema),
                description=description,
                required_text=required_text,
                enum_values=enum_values,
                children=children,
            )

        if schema.get("type") == "array" or "items" in schema:
            raw_items = schema.get("items")
            item_schema = raw_items if isinstance(raw_items, dict) else {}
            children = []
            if item_schema:
                children.append(cls._build_schema_node(field_name="项", field_schema=item_schema, required_text="可选"))
            return _SchemaFieldNode(
                name=field_name,
                type_name=f"array<{cls._schema_type_name(item_schema)}>" if item_schema else "array",
                description=description or "数组元素",
                required_text=required_text,
                enum_values=enum_values,
                children=children,
            )

        properties = schema.get("properties")
        if isinstance(properties, dict):
            required_fields = {str(item).strip() for item in schema.get("required", []) if str(item).strip()}
            children = [
                cls._build_schema_node(
                    field_name=str(key),
                    field_schema=value if isinstance(value, dict) else {},
                    required_text="必填" if str(key) in required_fields else "可选",
                )
                for key, value in properties.items()
            ]
            return _SchemaFieldNode(
                name=field_name,
                type_name=cls._schema_type_name(schema),
                description=description,
                required_text=required_text,
                enum_values=enum_values,
                children=children,
            )

        return _SchemaFieldNode(
            name=field_name,
            type_name=cls._schema_type_name(schema),
            description=description,
            required_text=required_text,
            enum_values=enum_values,
        )

    @staticmethod
    def _schema_description(schema: dict[str, Any]) -> str:
        base_description = str(schema.get("description") or schema.get("title") or "")
        enum_values = schema.get("enum")
        if isinstance(enum_values, list) and enum_values:
            return base_description
        return base_description

    @staticmethod
    def _schema_enum_values(schema: dict[str, Any]) -> list[str]:
        enum_values = schema.get("enum")
        if not isinstance(enum_values, list):
            return []
        return [str(item) for item in enum_values if item is not None]

    @staticmethod
    def _union_variants(schema: dict[str, Any]) -> list[Any]:
        for union_key in ("oneOf", "anyOf"):
            variants = schema.get(union_key)
            if isinstance(variants, list) and variants:
                return variants
        return []

    @classmethod
    def _merge_union_children(cls, variants: list[Any]) -> list[_SchemaFieldNode]:
        merged: dict[str, _SchemaFieldNode] = {}
        for item in variants:
            if not isinstance(item, dict):
                continue
            schema = cls._merge_all_of_schema(item)
            properties = schema.get("properties")
            if not isinstance(properties, dict):
                continue

            required_fields = {str(field).strip() for field in schema.get("required", []) if str(field).strip()}
            for key, value in properties.items():
                field_name = str(key)
                if field_name in merged:
                    continue
                merged[field_name] = cls._build_schema_node(
                    field_name=field_name,
                    field_schema=value if isinstance(value, dict) else {},
                    required_text="必填" if field_name in required_fields else "可选",
                )
        return list(merged.values())

    @classmethod
    def _merge_all_of_schema(cls, schema: dict[str, Any]) -> dict[str, Any]:
        all_of = schema.get("allOf")
        if not isinstance(all_of, list) or not all_of:
            return schema

        merged_properties: dict[str, Any] = {}
        merged_required: list[str] = []
        can_merge = True
        for item in all_of:
            if not isinstance(item, dict):
                can_merge = False
                break
            item_schema = cls._merge_all_of_schema(item)
            properties = item_schema.get("properties")
            if not isinstance(properties, dict):
                can_merge = False
                break
            merged_properties.update(properties)
            merged_required.extend(str(field) for field in item_schema.get("required", []) if str(field).strip())

        if not can_merge:
            return schema

        merged_schema = dict(schema)
        merged_schema["type"] = "object"
        merged_schema["properties"] = merged_properties
        merged_schema["required"] = list(dict.fromkeys(merged_required))
        merged_schema.pop("allOf", None)
        return merged_schema

    @staticmethod
    def _schema_type_name(field_schema: dict[str, Any]) -> str:
        if field_schema.get("const") is not None:
            return "value"
        type_name = field_schema.get("type")
        if isinstance(type_name, list):
            return " | ".join(str(item) for item in type_name if item)
        if isinstance(type_name, str) and type_name.strip():
            return type_name.strip()
        if isinstance(field_schema.get("allOf"), list):
            return "intersection"
        if isinstance(field_schema.get("enum"), list):
            enum_values = [item for item in field_schema.get("enum", []) if item is not None]
            if not enum_values:
                return "enum"
            if all(isinstance(item, str) for item in enum_values):
                return "enum<string>"
            if all(isinstance(item, bool) for item in enum_values):
                return "enum<boolean>"
            if all(isinstance(item, int | float) and not isinstance(item, bool) for item in enum_values):
                return "enum<number>"
            return "enum<any>"
        if isinstance(field_schema.get("oneOf"), list):
            return "oneOf"
        if isinstance(field_schema.get("anyOf"), list):
            return "anyOf"
        return "any"
