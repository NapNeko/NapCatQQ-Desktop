# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CardWidget,
    FluentIcon,
    PrimaryPushButton,
    PushButton,
    SegmentedWidget,
    StrongBodyLabel,
    TextBrowser,
    TitleLabel,
    ToolTipFilter,
)

from src.core.api_debug import ApiDebugExecutionResult
from src.ui.components.code_editor.editor import JsonEditor
from src.ui.components.code_editor.exhibit import CodeExibit
from src.ui.components.skeleton_widget import SkeletonShape, SkeletonWidget
from src.ui.components.stacked_widget import TransparentStackedWidget
from ..shared import ApiDebugChip, pretty_json


class ActionResultCard(QWidget):
    """执行结果区域。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.title_label = StrongBodyLabel("执行结果", self)
        self.status_chip = ApiDebugChip("未执行", self)
        self.meta_label = CaptionLabel("发送后会在这里展示本次调用结果", self)
        self.meta_label.setWordWrap(True)
        self.content_view = CodeExibit(self)
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


class ActionDetailPanel(CardWidget):
    """右侧接口详情区。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugMainCard")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)

        self.detail_state_stack = TransparentStackedWidget(self)
        self.detail_state_stack.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.detail_content_page = QWidget(self.detail_state_stack)
        self.detail_loading_page = QWidget(self.detail_state_stack)
        self.detail_empty_page = QWidget(self.detail_state_stack)
        self.detail_state_stack.addWidget(self.detail_content_page)
        self.detail_state_stack.addWidget(self.detail_loading_page)
        self.detail_state_stack.addWidget(self.detail_empty_page)

        self.action_title = TitleLabel("选择接口", self.detail_content_page)
        self.action_summary = CaptionLabel("选择左侧接口后可查看参数、文档并执行调试", self.detail_content_page)
        self.action_summary.setWordWrap(True)
        self.generate_button = PushButton(FluentIcon.ROTATE, "生成预设参数", self.detail_content_page)
        self.send_button = PrimaryPushButton(FluentIcon.SEND, "发送调试请求", self.detail_content_page)

        header_widget = QWidget(self.detail_content_page)
        header_layout = QVBoxLayout(header_widget)
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)

        action_buttons_layout = QHBoxLayout()
        action_buttons_layout.setContentsMargins(0, 0, 0, 0)
        action_buttons_layout.setSpacing(8)
        action_buttons_layout.addWidget(self.generate_button)
        action_buttons_layout.addWidget(self.send_button)

        title_row_layout = QHBoxLayout()
        title_row_layout.setContentsMargins(0, 0, 0, 0)
        title_row_layout.setSpacing(12)
        title_row_layout.addWidget(self.action_title, 1, Qt.AlignmentFlag.AlignVCenter)
        title_row_layout.addLayout(action_buttons_layout, 0)

        header_layout.addLayout(title_row_layout)
        header_layout.addWidget(self.action_summary)

        self.detail_pivot = SegmentedWidget(self.detail_content_page)
        self.detail_stack = TransparentStackedWidget(self.detail_content_page)
        self.detail_stack.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.params_page = QWidget(self.detail_stack)
        self.docs_page = QWidget(self.detail_stack)
        self.result_page = QWidget(self.detail_stack)
        self.detail_stack.addWidget(self.params_page)
        self.detail_stack.addWidget(self.docs_page)
        self.detail_stack.addWidget(self.result_page)
        self.detail_pivot.addItem("params", "参数", lambda: self.show_detail_page("params"))
        self.detail_pivot.addItem("docs", "文档", lambda: self.show_detail_page("docs"))
        self.detail_pivot.addItem("result", "结果", lambda: self.show_detail_page("result"))

        self.params_label = StrongBodyLabel("参数编辑", self.params_page)
        self.params_hint = CaptionLabel("优先使用 payloadExample，没有示例时按 schema 自动生成默认 JSON", self.params_page)
        self.params_hint.setWordWrap(True)
        self.params_editor = JsonEditor(self.params_page)

        params_layout = QVBoxLayout(self.params_page)
        params_layout.setContentsMargins(0, 0, 0, 0)
        params_layout.setSpacing(10)
        params_layout.addWidget(self.params_label)
        params_layout.addWidget(self.params_hint)
        params_layout.addWidget(self.params_editor, 1)

        self.docs_label = StrongBodyLabel("接口文档", self.docs_page)
        self.docs_view = TextBrowser(self.docs_page)
        self.docs_view.setPlainText("选择接口后可查看说明与请求/返回 schema。")

        docs_layout = QVBoxLayout(self.docs_page)
        docs_layout.setContentsMargins(0, 0, 0, 0)
        docs_layout.setSpacing(10)
        docs_layout.addWidget(self.docs_label)
        docs_layout.addWidget(self.docs_view, 1)

        self.result_card = ActionResultCard(self.result_page)
        result_layout = QVBoxLayout(self.result_page)
        result_layout.setContentsMargins(0, 0, 0, 0)
        result_layout.setSpacing(0)
        result_layout.addWidget(self.result_card, 1)

        content_layout = QVBoxLayout(self.detail_content_page)
        content_layout.setContentsMargins(20, 18, 20, 18)
        content_layout.setSpacing(10)
        content_layout.addWidget(header_widget)
        content_layout.addWidget(self.detail_pivot)
        content_layout.addWidget(self.detail_stack, 1)

        self.loading_title = BodyLabel("正在加载接口", self.detail_loading_page)
        self.loading_hint = CaptionLabel(
            "正在从当前 Bot 的 WebUI Debug 接口获取 Action schema，请稍候。",
            self.detail_loading_page,
        )
        self.loading_hint.setWordWrap(True)
        self.loading_hint.setAlignment(Qt.AlignmentFlag.AlignLeft)
        self.loading_skeleton = SkeletonWidget(self._build_loading_skeleton_shapes, self.detail_loading_page, panel_margin=0)
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

        self._page_map = {
            "params": self.params_page,
            "docs": self.docs_page,
            "result": self.result_page,
        }
        self._setup_tooltips()
        self.show_detail_page("params")

    def show_detail_page(self, route_key: str) -> None:
        widget = self._page_map[route_key]
        self.detail_stack.setCurrentWidget(widget)
        self.detail_pivot.setCurrentItem(route_key)

    def set_empty_state(self, title: str, message: str) -> None:
        self.empty_title.setText(title)
        self.empty_hint.setText(message)
        self.detail_state_stack.setCurrentWidget(self.detail_empty_page)

    def set_loading_state(self, title: str, message: str) -> None:
        self.loading_title.setText(title)
        self.loading_hint.setText(message)
        self.detail_state_stack.setCurrentWidget(self.detail_loading_page)

    def set_enabled_state(self, enabled: bool) -> None:
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
        self.generate_button.setToolTip("根据当前 schema 自动生成一份默认参数")
        self.send_button.setToolTip("向当前 Bot 的调试接口发送本次调用")
        self.detail_pivot.setToolTip("切换参数、文档和结果视图")

        for widget in [self.generate_button, self.send_button, self.detail_pivot]:
            widget.setToolTipDuration(1000)
            widget.installEventFilter(ToolTipFilter(widget, showDelay=300))
