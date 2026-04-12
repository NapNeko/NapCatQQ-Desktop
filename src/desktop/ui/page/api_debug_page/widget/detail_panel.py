# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Qt, Slot
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QFrame, QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CardWidget,
    PrimaryPushButton,
    ScrollArea,
    StrongBodyLabel,
    TitleLabel,
    ToolTipFilter,
)
from qfluentwidgets.common.smooth_scroll import SmoothMode

from src.desktop.core.api_debug import ApiDebugActionSchema
from src.desktop.ui.components.skeleton_widget import SkeletonShape, SkeletonWidget
from src.desktop.ui.components.stacked_widget import TransparentStackedWidget
from .debug_card import ActionDebugCard
from .method_badge import MethodBadge, apply_method_badge
from .schema_card import ApiDebugSchemaCard


class _ActionMetaCard(CardWidget):
    """请求方法与路由摘要卡片。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugStickyCard")

        self.method_chip = MethodBadge("POST", self)
        self.route_label = StrongBodyLabel("/action_name", self)
        self.route_label.setObjectName("ApiDebugRouteLabel")
        self.route_label.setWordWrap(True)
        self.debug_button = PrimaryPushButton("查看示例", self)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 8, 12, 8)
        layout.setSpacing(8)
        layout.addWidget(self.method_chip, 0, Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self.route_label, 1, Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self.debug_button, 0, Qt.AlignmentFlag.AlignVCenter)

    def set_method_style(self, method_text: str) -> None:
        apply_method_badge(self.method_chip, method_text)

    def set_route(self, route_text: str) -> None:
        self.route_label.setText(route_text)


class ActionDetailPanel(QWidget):
    """右侧接口详情区。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugDetailPanel")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self._debug_panel_collapsed_width = 0
        self._debug_panel_min_width = 400
        self._debug_panel_max_width = 460

        self.detail_state_stack = TransparentStackedWidget(self)
        self.detail_state_stack.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.detail_content_page = QWidget(self.detail_state_stack)
        self.detail_loading_page = QWidget(self.detail_state_stack)
        self.detail_empty_page = QWidget(self.detail_state_stack)
        self.detail_state_stack.addWidget(self.detail_content_page)
        self.detail_state_stack.addWidget(self.detail_loading_page)
        self.detail_state_stack.addWidget(self.detail_empty_page)

        self.category_label = TitleLabel("分组名称", self.detail_content_page)
        self.category_label.setObjectName("ApiDebugGroupLabel")
        self.category_label.setWordWrap(True)
        self.action_title = BodyLabel("选择接口", self.detail_content_page)
        self.action_title.setObjectName("ApiDebugActionName")
        self.action_title.setWordWrap(True)
        self._apply_header_label_fonts()

        self.sticky_meta_card = _ActionMetaCard(self.detail_content_page)
        self.pinned_meta_card = _ActionMetaCard(self.detail_content_page)
        self.request_method_chip = self.sticky_meta_card.method_chip
        self.request_route_label = self.sticky_meta_card.route_label
        self.debug_button = self.sticky_meta_card.debug_button
        self.pinned_request_method_chip = self.pinned_meta_card.method_chip
        self.pinned_request_route_label = self.pinned_meta_card.route_label
        self.pinned_debug_button = self.pinned_meta_card.debug_button

        self.docs_scroll = ScrollArea(self.detail_content_page)
        self.docs_scroll.setObjectName("ApiDebugDocsScroll")
        self.docs_container = QWidget(self.docs_scroll)
        self.docs_container.setObjectName("ApiDebugDocsContainer")
        self.docs_scroll.setWidget(self.docs_container)
        self.docs_scroll.setWidgetResizable(True)
        self.docs_scroll.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        self.docs_scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.docs_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.docs_scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.docs_scroll.setSmoothMode(SmoothMode.LINEAR, Qt.Orientation.Vertical)
        self.docs_scroll.enableTransparentBackground()

        self.request_params_label = StrongBodyLabel("请求参数", self.docs_container)
        self.body_params_card = ApiDebugSchemaCard("Body 参数", include_example=True, parent=self.docs_container)
        self.response_card = ApiDebugSchemaCard("返回响应", parent=self.docs_container)

        self.body_header_widget = self.body_params_card.header_widget
        self.body_params_title = self.body_params_card.title_label
        self.body_content_type_chip = self.body_params_card.content_type_chip
        self.body_header_divider = self.body_params_card.header_divider
        self.body_rows_widget = self.body_params_card.body_widget
        self.params_rows_container = self.body_params_card.rows_container
        self.params_rows_layout = self.body_params_card.rows_layout
        self.example_bar = self.body_params_card.example_bar
        self.example_toggle_button = self.body_params_card.example_toggle_button
        self.example_footer_divider = self.body_params_card.example_footer_divider
        self.example_view = self.body_params_card.example_view

        self.response_header_widget = self.response_card.header_widget
        self.response_title = self.response_card.title_label
        self.response_content_type_chip = self.response_card.content_type_chip
        self.response_header_divider = self.response_card.header_divider
        self.response_body_widget = self.response_card.body_widget
        self.response_rows_container = self.response_card.rows_container
        self.response_rows_layout = self.response_card.rows_layout
        self.response_schema_view = self.response_card.schema_preview

        docs_layout = QVBoxLayout(self.docs_container)
        docs_layout.setContentsMargins(0, 0, 10, 0)
        docs_layout.setSpacing(4)
        docs_layout.addWidget(self.category_label)
        docs_layout.addWidget(self.action_title)
        docs_layout.addWidget(self.sticky_meta_card)
        docs_layout.addSpacing(4)
        docs_layout.addWidget(self.request_params_label)
        docs_layout.addWidget(self.body_params_card)
        docs_layout.addSpacing(4)
        docs_layout.addWidget(self.response_card)
        docs_layout.addStretch(1)

        self.debug_panel_container = QWidget(self.detail_content_page)
        self.debug_panel_container.setObjectName("ApiDebugDebugPanelContainer")
        self.debug_panel_container.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)

        self.debug_card = ActionDebugCard(self.debug_panel_container)
        self.debug_card.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.debug_close_button = self.debug_card.debug_close_button
        self.generate_button = self.debug_card.generate_button
        self.send_button = self.debug_card.send_button
        self.params_label = self.debug_card.params_label
        self.params_editor = self.debug_card.params_editor
        self.result_card = self.debug_card.result_card
        self.runtime_route_label = self.debug_card.runtime_meta_card.route_label
        self.runtime_method_chip = self.debug_card.runtime_meta_card.method_chip

        debug_panel_layout = QVBoxLayout(self.debug_panel_container)
        debug_panel_layout.setContentsMargins(8, 8, 6, 12)
        debug_panel_layout.setSpacing(0)
        debug_panel_layout.addWidget(self.debug_card, 1)

        left_column = QWidget(self.detail_content_page)
        left_column.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        left_column_layout = QVBoxLayout(left_column)
        left_column_layout.setContentsMargins(0, 0, 0, 0)
        left_column_layout.setSpacing(8)
        left_column_layout.addWidget(self.pinned_meta_card)
        left_column_layout.addWidget(self.docs_scroll, 1)

        body_layout = QHBoxLayout()
        body_layout.setContentsMargins(0, 0, 0, 0)
        body_layout.setSpacing(24)
        body_layout.addWidget(left_column, 1)
        body_layout.addWidget(self.debug_panel_container, 0)

        content_layout = QVBoxLayout(self.detail_content_page)
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(0)
        content_layout.addLayout(body_layout, 1)

        self.loading_title = BodyLabel("正在加载接口", self.detail_loading_page)
        self.loading_hint = CaptionLabel(
            "正在从当前 Bot 的 WebUI Debug 接口获取 Action schema，请稍候。",
            self.detail_loading_page,
        )
        self.loading_hint.setWordWrap(True)
        self.loading_hint.setAlignment(Qt.AlignmentFlag.AlignLeft)
        self.loading_skeleton = SkeletonWidget(
            self._build_loading_skeleton_shapes, self.detail_loading_page, panel_margin=0
        )
        self.loading_skeleton.setMinimumHeight(220)
        self.loading_skeleton.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        loading_layout = QVBoxLayout(self.detail_loading_page)
        loading_layout.setContentsMargins(28, 24, 28, 24)
        loading_layout.setSpacing(12)
        loading_layout.addWidget(self.loading_title)
        loading_layout.addWidget(self.loading_hint)
        loading_layout.addWidget(self.loading_skeleton, 1)

        self.empty_title = StrongBodyLabel("选择一个接口查看文档", self.detail_empty_page)
        self.empty_title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.empty_hint = CaptionLabel("左侧会展示当前 Bot 可用的 Action 接口与简要说明。", self.detail_empty_page)
        self.empty_hint.setWordWrap(True)
        self.empty_hint.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.empty_container = QWidget(self.detail_empty_page)
        self.empty_container.setMinimumWidth(360)
        self.empty_container.setMaximumWidth(520)
        self.empty_container.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Maximum)

        empty_container_layout = QVBoxLayout(self.empty_container)
        empty_container_layout.setContentsMargins(24, 20, 24, 20)
        empty_container_layout.setSpacing(12)
        empty_container_layout.addWidget(self.empty_title)
        empty_container_layout.addWidget(self.empty_hint)

        empty_layout = QVBoxLayout(self.detail_empty_page)
        empty_layout.setContentsMargins(24, 24, 24, 24)
        empty_layout.setSpacing(8)
        empty_layout.addStretch(1)
        empty_layout.addWidget(self.empty_container, 0, Qt.AlignmentFlag.AlignHCenter)
        empty_layout.addStretch(1)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        layout.addWidget(self.detail_state_stack, 1)

        self._setup_tooltips()
        self.debug_button.clicked.connect(self.show_debug_panel)
        self.pinned_debug_button.clicked.connect(self.show_debug_panel)
        self.example_toggle_button.clicked.connect(self._toggle_example_card)
        self.docs_scroll.verticalScrollBar().valueChanged.connect(self._sync_pinned_meta_card)
        self._set_example_visible(False)
        self.pinned_meta_card.hide()
        self._set_debug_panel_visible(False)

    def apply_schema(
        self,
        schema: ApiDebugActionSchema,
        *,
        category_name: str,
        display_name: str,
        example_text: str,
        request_method: str,
    ) -> None:
        self.category_label.setText(category_name)
        self.action_title.setText(display_name)
        self._apply_request_method(request_method)
        route_text = f"/{schema.action}"
        self.sticky_meta_card.set_route(route_text)
        self.pinned_meta_card.set_route(route_text)
        self.debug_card.runtime_meta_card.set_route(route_text)
        self._rebuild_param_rows(schema.payload_schema)
        wrapped_response_schema = self._build_response_schema(schema.return_schema)
        self._rebuild_response_rows(wrapped_response_schema)
        self.body_params_card.set_example_text(example_text or "{}")
        self.response_card.set_schema_preview(wrapped_response_schema)
        self._set_example_visible(False)
        self._sync_pinned_meta_card()
        self.hide_debug_panel()

    @Slot()
    def show_debug_panel(self) -> None:
        self._set_debug_panel_visible(True)
        if self.params_editor.isEnabled():
            self.params_editor.setFocus(Qt.FocusReason.OtherFocusReason)

    @Slot()
    def hide_debug_panel(self) -> None:
        self._set_debug_panel_visible(False)

    def resizeEvent(self, event) -> None:  # noqa: ANN001
        super().resizeEvent(event)
        if not self.debug_card.isHidden():
            self.debug_panel_container.setFixedWidth(self._debug_panel_target_width())

    def _set_debug_panel_visible(self, visible: bool) -> None:
        target_width = self._debug_panel_target_width() if visible else self._debug_panel_collapsed_width
        self.debug_panel_container.setMinimumWidth(target_width)
        self.debug_panel_container.setMaximumWidth(target_width)
        self.debug_panel_container.setFixedWidth(target_width)
        self.debug_panel_container.setVisible(visible)
        self.debug_card.setVisible(visible)
        self._refresh_body_layout()

    def _debug_panel_target_width(self) -> int:
        available_width = max(self.width(), self.detail_state_stack.width(), self.detail_content_page.width())
        if available_width <= 0:
            return self._debug_panel_min_width
        return max(self._debug_panel_min_width, min(self._debug_panel_max_width, int(available_width * 0.36)))

    def _refresh_body_layout(self) -> None:
        parent_layout = self.detail_content_page.layout()
        if parent_layout is not None:
            parent_layout.invalidate()
            parent_layout.activate()
        self.updateGeometry()

    def _apply_request_method(self, request_method: str) -> None:
        method_text = (request_method or "POST").upper()
        self.sticky_meta_card.set_method_style(method_text)
        self.pinned_meta_card.set_method_style(method_text)
        self.debug_card.runtime_meta_card.set_method_style(method_text)

    def _sync_pinned_meta_card(self, *_args) -> None:
        threshold = max(0, self.sticky_meta_card.y())
        should_pin = self.docs_scroll.verticalScrollBar().value() > threshold
        self.pinned_meta_card.setVisible(should_pin)

    def _apply_header_label_fonts(self) -> None:
        group_font = QFont(self.category_label.font())
        group_font.setPointSizeF(12)
        group_font.setLetterSpacing(QFont.SpacingType.PercentageSpacing, 110)
        self.category_label.setFont(group_font)

        action_font = QFont(self.action_title.font())
        action_font.setPointSizeF(20)
        action_font.setWeight(QFont.Weight.Normal)
        action_font.setLetterSpacing(QFont.SpacingType.PercentageSpacing, 115)
        self.action_title.setFont(action_font)

    def _set_example_visible(self, visible: bool) -> None:
        self.body_params_card.set_example_visible(visible)

    @Slot()
    def _toggle_example_card(self) -> None:
        self.body_params_card.toggle_example()

    def _rebuild_param_rows(self, payload_schema) -> None:
        self.body_params_card.rebuild_rows(payload_schema, empty_text="当前接口没有可展示的 Body 字段定义。")

    def _rebuild_response_rows(self, response_schema) -> None:
        self.response_card.rebuild_rows(
            response_schema,
            empty_text="当前接口没有可展示的返回字段定义。",
            show_schema_preview_when_needed=True,
        )

    @staticmethod
    def _build_response_schema(data_schema) -> dict:
        if isinstance(data_schema, dict):
            merged_schema = ActionDetailPanel._merge_response_schema(data_schema)
            properties = merged_schema.get("properties")
            if isinstance(properties, dict) and any(
                key in properties for key in ["status", "retcode", "data", "message", "wording", "stream"]
            ):
                return merged_schema

        return {
            "type": "object",
            "properties": {
                "status": {
                    "oneOf": [
                        {"const": "ok", "description": "状态 (ok/failed)"},
                        {"const": "failed", "description": "状态 (ok/failed)"},
                    ],
                    "description": "状态",
                },
                "retcode": {"type": "integer", "description": "返回码"},
                "data": data_schema if isinstance(data_schema, dict) else {"type": "any", "description": "数据"},
                "message": {"type": "string", "description": "消息"},
                "wording": {"type": "string", "description": "提示"},
                "echo": {"type": "string", "description": "回显"},
                "stream": {
                    "type": "string",
                    "description": "流式响应",
                    "enum": ["stream-action", "normal-action"],
                },
            },
            "required": ["status", "retcode", "data", "message", "wording"],
        }

    @staticmethod
    def _merge_response_schema(schema: dict) -> dict:
        all_of = schema.get("allOf")
        if not isinstance(all_of, list) or not all_of:
            return schema

        merged_schema = dict(schema)
        merged_properties: dict = {}
        merged_required: list[str] = []
        merged = False

        for item in all_of:
            if not isinstance(item, dict):
                continue
            item_schema = ActionDetailPanel._merge_response_schema(item)
            properties = item_schema.get("properties")
            if isinstance(properties, dict):
                merged = True
                merged_properties.update(properties)
                merged_required.extend(str(field) for field in item_schema.get("required", []) if str(field).strip())

        if merged:
            merged_schema["type"] = "object"
            merged_schema["properties"] = merged_properties
            merged_schema["required"] = list(dict.fromkeys(merged_required))
            merged_schema.pop("allOf", None)

        return merged_schema

    def set_empty_state(self, title: str, message: str) -> None:
        self.hide_debug_panel()
        self.empty_title.setText(title)
        self.empty_hint.setText(message)
        self.detail_state_stack.setCurrentWidget(self.detail_empty_page)

    def set_loading_state(self, title: str, message: str) -> None:
        self.hide_debug_panel()
        self.loading_title.setText(title)
        self.loading_hint.setText(message)
        self.detail_state_stack.setCurrentWidget(self.detail_loading_page)

    def set_enabled_state(self, enabled: bool) -> None:
        self.debug_button.setEnabled(enabled)
        self.pinned_debug_button.setEnabled(enabled)
        self.debug_card.set_controls_enabled(enabled)
        if enabled:
            self.detail_state_stack.setCurrentWidget(self.detail_content_page)

    def _build_loading_skeleton_shapes(self, widget: QWidget) -> list[SkeletonShape]:
        width = max(320, widget.width() - 56)
        x = 0
        y = 8
        shapes: list[SkeletonShape] = [
            SkeletonShape(x, y, int(width * 0.22), 18, 1.08),
            SkeletonShape(x, y + 34, int(width * 0.54), 12, 0.96),
            SkeletonShape(x, y + 58, int(width * 0.38), 12, 0.92),
            SkeletonShape(x, y + 98, int(width * 0.96), 40, 1.02, 12),
            SkeletonShape(x, y + 156, int(width * 0.96), 220, 1.08, 16),
        ]
        return shapes

    def _setup_tooltips(self) -> None:
        if self.example_toggle_button is None:
            return

        self.debug_button.setToolTip("打开右侧接口示例面板")
        self.generate_button.setToolTip("根据当前 schema 自动生成一份默认参数")
        self.send_button.setToolTip("向当前 Bot 调用一次当前接口")
        self.debug_close_button.setToolTip("收起右侧接口示例面板")
        self.example_toggle_button.setToolTip("展开或收起请求参数示例")

        for widget in [
            self.debug_button,
            self.generate_button,
            self.send_button,
            self.debug_close_button,
            self.example_toggle_button,
        ]:
            widget.setToolTipDuration(1000)
            widget.installEventFilter(ToolTipFilter(widget, showDelay=300))
