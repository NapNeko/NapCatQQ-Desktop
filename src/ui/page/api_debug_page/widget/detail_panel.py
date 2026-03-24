# -*- coding: utf-8 -*-
from __future__ import annotations

from typing import Any

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QFont, QPainter, QPen
from PySide6.QtWidgets import QFrame, QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CardWidget,
    FluentIcon,
    PrimaryPushButton,
    PushButton,
    ScrollArea,
    StrongBodyLabel,
    TitleLabel,
    ToolButton,
    ToolTipFilter,
)
from qfluentwidgets.common.smooth_scroll import SmoothMode

from src.core.api_debug import ApiDebugActionSchema, ApiDebugExecutionResult
from src.core.config import cfg
from src.ui.components.code_editor.editor import JsonEditor
from src.ui.components.code_editor.exhibit import CodeExibit
from src.ui.components.skeleton_widget import SkeletonShape, SkeletonWidget
from src.ui.components.stacked_widget import TransparentStackedWidget
from ..shared import ApiDebugChip, pretty_json


class _ApiDebugParamRow(QWidget):
    """参数行。"""

    def __init__(
        self, name: str, type_name: str, description: str, required_text: str, parent: QWidget | None = None
    ) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugParamRow")

        self.name_label = CaptionLabel(name, self)
        self.name_label.setObjectName("ApiDebugParamName")
        self.type_label = CaptionLabel(type_name, self)
        self.type_label.setObjectName("ApiDebugParamType")
        self.description_label = CaptionLabel(description or "-", self)
        self.description_label.setObjectName("ApiDebugParamDescription")
        self.required_label = CaptionLabel(required_text, self)
        self.required_label.setObjectName("ApiDebugParamRequired")

        self._apply_name_chip_style()

        left_layout = QHBoxLayout()
        left_layout.setContentsMargins(0, 0, 0, 0)
        left_layout.setSpacing(8)
        left_layout.addWidget(self.name_label, 0)
        left_layout.addWidget(self.type_label, 0)
        left_layout.addWidget(self.description_label, 0)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(8)
        layout.addLayout(left_layout, 0)
        layout.addStretch(1)
        layout.addWidget(self.required_label, 0)

    def _apply_name_chip_style(self) -> None:
        color = QColor(cfg.get(cfg.themeColor))
        if not color.isValid():
            color = QColor("#2563eb")
        text_color = color.darker(120).name()
        fill_color = QColor(color)
        fill_color.setAlpha(26)
        border_color = QColor(color)
        border_color.setAlpha(58)
        self.name_label.setStyleSheet(
            "QLabel {"
            f"color: {text_color};"
            f"background: {fill_color.name(QColor.NameFormat.HexArgb)};"
            f"border: 1px solid {border_color.name(QColor.NameFormat.HexArgb)};"
            "border-radius: 6px;"
            "padding: 2px 8px;"
            "font-weight: 600;"
            "}"
        )


class _CardDivider(QWidget):
    """使用绘制事件保证可见的细分割线。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setFixedHeight(2)

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        pen = QPen(QColor(127, 127, 127, 96))
        pen.setWidth(1)
        painter.setPen(pen)
        y = self.height() / 2
        painter.drawLine(0, int(y), self.width(), int(y))


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


class ActionResultCard(QWidget):
    """执行结果区域。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.title_label = StrongBodyLabel("执行结果", self)
        self.status_chip = ApiDebugChip("未执行", self)
        self.meta_label = CaptionLabel("发送后会在这里展示本次调用结果", self)
        self.meta_label.setWordWrap(True)
        self.content_view = CodeExibit(self)
        self.content_view.setMinimumHeight(220)
        self.content_view.setPlainText("等待一次执行")

        header = QWidget(self)
        header_layout = QHBoxLayout(header)
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)
        header_layout.addWidget(self.title_label)
        header_layout.addStretch(1)
        header_layout.addWidget(self.status_chip)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(10)
        layout.addWidget(header)
        layout.addWidget(self.meta_label)
        layout.addWidget(self.content_view, 1)

    def reset(self, message: str = "等待一次执行") -> None:
        self.status_chip.set_text("未执行")
        self.meta_label.setText("发送后会在这里展示本次调用结果")
        self.content_view.setPlainText(message)

    def apply_result(self, result: ApiDebugExecutionResult | None) -> None:
        if result is None:
            self.reset()
            return

        if result.response is not None:
            self.status_chip.set_text(str(result.response.status_code))
            parts = [
                result.response.reason_phrase or "",
                f"{result.response.elapsed_ms:.2f} ms",
                self._format_size(result.response.size_bytes),
            ]
            self.meta_label.setText(" · ".join(part for part in parts if part))
            self.content_view.setPlainText(result.response.formatted_body or "响应体为空")
        else:
            self.status_chip.set_text("失败")
            self.meta_label.setText("调用没有返回可展示的响应")
            self.content_view.setPlainText("没有响应体")

        if result.error is not None:
            payload = {
                "kind": result.error.kind.value,
                "message": result.error.message,
            }
            if result.error.status_code is not None:
                payload["status_code"] = result.error.status_code
            if result.error.details:
                payload["details"] = result.error.details
            self.content_view.setPlainText(pretty_json(payload))

    @staticmethod
    def _format_size(size_bytes: int) -> str:
        if size_bytes < 1024:
            return f"{size_bytes} B"
        if size_bytes < 1024 * 1024:
            return f"{size_bytes / 1024:.1f} KB"
        return f"{size_bytes / (1024 * 1024):.1f} MB"


class ActionDetailPanel(QWidget):
    """右侧接口详情区。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugDetailPanel")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)

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

        self.sticky_meta_card = CardWidget(self.detail_content_page)
        self.sticky_meta_card.setObjectName("ApiDebugStickyCard")
        self.request_method_chip = CaptionLabel("POST", self.sticky_meta_card)
        self.request_method_chip.setObjectName("ApiDebugMethodChip")
        self.request_route_label = StrongBodyLabel("//action_name", self.sticky_meta_card)
        self.request_route_label.setObjectName("ApiDebugRouteLabel")
        self.request_route_label.setWordWrap(True)
        self.debug_button = PrimaryPushButton("调试", self.sticky_meta_card)

        self.pinned_meta_card = CardWidget(self.detail_content_page)
        self.pinned_meta_card.setObjectName("ApiDebugStickyCard")
        self.pinned_request_method_chip = CaptionLabel("POST", self.pinned_meta_card)
        self.pinned_request_method_chip.setObjectName("ApiDebugMethodChip")
        self.pinned_request_route_label = StrongBodyLabel("/action_name", self.pinned_meta_card)
        self.pinned_request_route_label.setObjectName("ApiDebugRouteLabel")
        self.pinned_request_route_label.setWordWrap(True)
        self.pinned_debug_button = PrimaryPushButton("调试", self.pinned_meta_card)

        sticky_layout = QHBoxLayout(self.sticky_meta_card)
        sticky_layout.setContentsMargins(12, 8, 12, 8)
        sticky_layout.setSpacing(8)
        sticky_layout.addWidget(self.request_method_chip, 0, Qt.AlignmentFlag.AlignVCenter)
        sticky_layout.addWidget(self.request_route_label, 1, Qt.AlignmentFlag.AlignVCenter)
        sticky_layout.addWidget(self.debug_button, 0, Qt.AlignmentFlag.AlignVCenter)

        pinned_layout = QHBoxLayout(self.pinned_meta_card)
        pinned_layout.setContentsMargins(12, 8, 12, 8)
        pinned_layout.setSpacing(8)
        pinned_layout.addWidget(self.pinned_request_method_chip, 0, Qt.AlignmentFlag.AlignVCenter)
        pinned_layout.addWidget(self.pinned_request_route_label, 1, Qt.AlignmentFlag.AlignVCenter)
        pinned_layout.addWidget(self.pinned_debug_button, 0, Qt.AlignmentFlag.AlignVCenter)

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

        self.body_params_card = CardWidget(self.docs_container)
        self.body_params_card.setObjectName("ApiDebugSchemaCard")
        self.body_content_widget = QWidget(self.body_params_card)
        self.body_content_widget.setObjectName("ApiDebugSchemaContent")
        self.body_params_title = StrongBodyLabel("Body 参数", self.body_content_widget)
        self.body_content_type_chip = CaptionLabel("application/json", self.body_content_widget)
        self.body_content_type_chip.setObjectName("ApiDebugContentTypeChip")
        self.body_header_divider = _CardDivider(self.body_params_card)
        self.body_header_divider.setObjectName("ApiDebugBodyDivider")
        self.params_rows_container = QWidget(self.body_content_widget)
        self.params_rows_layout = QVBoxLayout(self.params_rows_container)
        self.params_rows_layout.setContentsMargins(0, 0, 0, 0)
        self.params_rows_layout.setSpacing(6)
        self.example_bar = _ExampleFooterBar(self.body_params_card)
        self.example_bar.setObjectName("ApiDebugExampleBar")
        self.example_toggle_button = PushButton("示例  ▼", self.example_bar)
        self.example_toggle_button.setObjectName("ApiDebugExampleToggle")
        self.example_toggle_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.example_footer_divider = _CardDivider(self.example_bar)
        self.example_footer_divider.setObjectName("ApiDebugBodyDivider")
        self.example_view = CodeExibit(self.body_params_card)
        self.example_view.setMinimumHeight(160)

        body_header_layout = QHBoxLayout()
        body_header_layout.setContentsMargins(0, 0, 0, 0)
        body_header_layout.setSpacing(8)
        body_header_layout.addWidget(self.body_params_title)
        body_header_layout.addWidget(self.body_content_type_chip, 0, Qt.AlignmentFlag.AlignVCenter)
        body_header_layout.addStretch(1)

        body_content_layout = QVBoxLayout(self.body_content_widget)
        body_content_layout.setContentsMargins(12, 12, 12, 12)
        body_content_layout.setSpacing(10)
        body_content_layout.addLayout(body_header_layout)
        body_content_layout.addWidget(self.params_rows_container)

        example_bar_layout = QVBoxLayout(self.example_bar)
        example_bar_layout.setContentsMargins(0, 0, 0, 0)
        example_bar_layout.setSpacing(0)
        example_bar_layout.addWidget(self.example_footer_divider)
        example_bar_layout.addWidget(self.example_toggle_button)

        body_card_layout = QVBoxLayout(self.body_params_card)
        body_card_layout.setContentsMargins(0, 0, 0, 0)
        body_card_layout.setSpacing(0)
        body_card_layout.addWidget(self.body_content_widget)
        body_card_layout.addWidget(self.body_header_divider)
        body_card_layout.addWidget(self.example_bar)
        body_card_layout.addWidget(self.example_view)

        self.response_label = StrongBodyLabel("返回响应", self.docs_container)
        self.response_card = CardWidget(self.docs_container)
        self.response_card.setObjectName("ApiDebugSchemaCard")
        self.response_view = CodeExibit(self.response_card)
        self.response_view.setMinimumHeight(220)
        self.response_view.setPlainText("{}")

        response_card_layout = QVBoxLayout(self.response_card)
        response_card_layout.setContentsMargins(12, 12, 12, 12)
        response_card_layout.setSpacing(0)
        response_card_layout.addWidget(self.response_view)

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
        docs_layout.addWidget(self.response_label)
        docs_layout.addWidget(self.response_card)
        docs_layout.addStretch(1)

        self.debug_card = CardWidget(self.detail_content_page)
        self.debug_card.setObjectName("ApiDebugDebugCard")
        self.debug_card.setMinimumWidth(400)
        self.debug_card.setMaximumWidth(460)
        self.debug_card.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Expanding)
        self.debug_close_button = ToolButton(FluentIcon.CLOSE, self.debug_card)
        self.generate_button = PushButton(FluentIcon.ROTATE, "生成预设参数", self.debug_card)
        self.send_button = PrimaryPushButton(FluentIcon.SEND, "发送调试请求", self.debug_card)
        self.params_label = StrongBodyLabel("调试参数", self.debug_card)
        self.params_hint = CaptionLabel(
            "优先使用 payloadExample，没有示例时按 schema 自动生成默认 JSON", self.debug_card
        )
        self.params_hint.setWordWrap(True)
        self.params_editor = JsonEditor(self.debug_card)
        self.params_editor.setMinimumHeight(240)
        self.result_card = ActionResultCard(self.debug_card)

        debug_header = QWidget(self.debug_card)
        debug_header_layout = QHBoxLayout(debug_header)
        debug_header_layout.setContentsMargins(0, 0, 0, 0)
        debug_header_layout.setSpacing(8)
        debug_header_layout.addWidget(StrongBodyLabel("调试与结果", debug_header))
        debug_header_layout.addStretch(1)
        debug_header_layout.addWidget(self.debug_close_button)

        debug_action_layout = QHBoxLayout()
        debug_action_layout.setContentsMargins(0, 0, 0, 0)
        debug_action_layout.setSpacing(8)
        debug_action_layout.addWidget(self.generate_button)
        debug_action_layout.addWidget(self.send_button)

        debug_layout = QVBoxLayout(self.debug_card)
        debug_layout.setContentsMargins(18, 18, 18, 18)
        debug_layout.setSpacing(12)
        debug_layout.addWidget(debug_header)
        debug_layout.addWidget(self.params_label)
        debug_layout.addWidget(self.params_hint)
        debug_layout.addWidget(self.params_editor)
        debug_layout.addLayout(debug_action_layout)
        debug_layout.addSpacing(4)
        debug_layout.addWidget(self.result_card, 1)

        left_column = QWidget(self.detail_content_page)
        left_column_layout = QVBoxLayout(left_column)
        left_column_layout.setContentsMargins(0, 0, 0, 0)
        left_column_layout.setSpacing(8)
        left_column_layout.addWidget(self.pinned_meta_card)
        left_column_layout.addWidget(self.docs_scroll, 1)

        body_layout = QHBoxLayout()
        body_layout.setContentsMargins(0, 0, 0, 0)
        body_layout.setSpacing(20)
        body_layout.addWidget(left_column, 1)
        body_layout.addWidget(self.debug_card, 0)

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

        self.empty_title = StrongBodyLabel("选择一个接口开始调试", self.detail_empty_page)
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
        self.hide_debug_panel()

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
        self.request_route_label.setText(route_text)
        self.pinned_request_route_label.setText(route_text)
        self._rebuild_param_rows(schema.payload_schema)
        self.response_view.setPlainText(
            pretty_json(
                schema.return_schema if schema.return_schema is not None else {"message": "暂无返回响应 Schema"}
            )
        )
        self.example_view.setPlainText(example_text or "{}")
        self._set_example_visible(False)
        self._sync_pinned_meta_card()
        self.hide_debug_panel()

    def show_debug_panel(self) -> None:
        self.debug_card.show()
        if self.params_editor.isEnabled():
            self.params_editor.setFocus(Qt.FocusReason.OtherFocusReason)

    def hide_debug_panel(self) -> None:
        self.debug_card.hide()

    def _apply_request_method(self, request_method: str) -> None:
        method_text = (request_method or "POST").upper()
        method_type = method_text.lower()
        background_map = {
            "get": "rgba(22, 163, 74, 0.96)",
            "post": "rgba(37, 99, 235, 0.98)",
            "put": "rgba(234, 88, 12, 0.98)",
            "delete": "rgba(220, 38, 38, 0.98)",
            "patch": "rgba(147, 51, 234, 0.98)",
        }
        background = background_map.get(method_type, "rgba(71, 85, 105, 0.96)")
        for label in [self.request_method_chip, self.pinned_request_method_chip]:
            label.setText(method_text)
            label.setProperty("methodType", method_type)
            label.setStyleSheet(
                "QLabel {"
                "color: white;"
                f"background: {background};"
                "border-radius: 4px;"
                "padding: 3px 8px;"
                "font-weight: 600;"
                "}"
            )

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
        self.example_view.setVisible(visible)
        self.example_toggle_button.setText("示例  ▲" if visible else "示例  ▼")

    def _toggle_example_card(self) -> None:
        self._set_example_visible(not self.example_view.isVisible())

    def _rebuild_param_rows(self, payload_schema: Any) -> None:
        while self.params_rows_layout.count():
            item = self.params_rows_layout.takeAt(0)
            if item is None:
                continue
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        fields = self._extract_param_fields(payload_schema)
        if not fields:
            self.params_rows_layout.addWidget(
                CaptionLabel("当前接口没有可展示的 Body 字段定义。", self.params_rows_container)
            )
            return

        for name, type_name, description, required_text in fields:
            self.params_rows_layout.addWidget(
                _ApiDebugParamRow(name, type_name, description, required_text, self.params_rows_container)
            )

    @classmethod
    def _extract_param_fields(cls, payload_schema: Any) -> list[tuple[str, str, str, str]]:
        if not isinstance(payload_schema, dict):
            return []

        properties = payload_schema.get("properties")
        if not isinstance(properties, dict):
            return []

        required_fields = {
            str(field_name).strip() for field_name in payload_schema.get("required", []) if str(field_name).strip()
        }
        fields: list[tuple[str, str, str, str]] = []
        for field_name, field_schema in properties.items():
            if not isinstance(field_schema, dict):
                field_schema = {}
            type_name = cls._schema_type_name(field_schema)
            description = str(field_schema.get("description") or field_schema.get("title") or "")
            required_text = "必填" if str(field_name) in required_fields else "可选"
            fields.append((str(field_name), type_name, description, required_text))
        return fields

    @staticmethod
    def _schema_type_name(field_schema: dict[str, Any]) -> str:
        type_name = field_schema.get("type")
        if isinstance(type_name, list):
            return " | ".join(str(item) for item in type_name if item)
        if isinstance(type_name, str) and type_name.strip():
            return type_name.strip()
        if isinstance(field_schema.get("enum"), list):
            return "enum"
        if isinstance(field_schema.get("oneOf"), list):
            return "oneOf"
        if isinstance(field_schema.get("anyOf"), list):
            return "anyOf"
        return "any"

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
        self.generate_button.setEnabled(enabled)
        self.send_button.setEnabled(enabled)
        self.params_editor.setEnabled(enabled)
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
        self.debug_button.setToolTip("打开右侧调试面板")
        self.generate_button.setToolTip("根据当前 schema 自动生成一份默认参数")
        self.send_button.setToolTip("向当前 Bot 的调试接口发送本次调用")
        self.debug_close_button.setToolTip("收起右侧调试面板")
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
