# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    CardWidget,
    FluentIcon,
    PrimaryPushButton,
    PushButton,
    SegmentedWidget,
    StrongBodyLabel,
    TextBrowser,
    TitleLabel,
)

from src.core.api_debug import ApiDebugExecutionResult
from src.ui.components.code_editor.editor import JsonEditor
from src.ui.components.code_editor.exhibit import CodeExibit
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
        self.detail_empty_page = QWidget(self.detail_state_stack)
        self.detail_state_stack.addWidget(self.detail_content_page)
        self.detail_state_stack.addWidget(self.detail_empty_page)

        self.action_title = TitleLabel("选择接口", self.detail_content_page)
        self.action_summary = CaptionLabel("从左侧接口目录选择一个 Action", self.detail_content_page)
        self.action_summary.setWordWrap(True)
        self.action_tags = CaptionLabel("", self.detail_content_page)
        self.action_tags.setWordWrap(True)
        self.generate_button = PushButton(FluentIcon.ROTATE, "生成预设参数", self.detail_content_page)
        self.send_button = PrimaryPushButton(FluentIcon.SEND, "发送调试请求", self.detail_content_page)

        info_header = QWidget(self.detail_content_page)
        info_actions = QHBoxLayout(info_header)
        info_actions.setContentsMargins(0, 0, 0, 0)
        info_actions.setSpacing(8)
        info_actions.addStretch(1)
        info_actions.addWidget(self.generate_button)
        info_actions.addWidget(self.send_button)

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
        self.docs_view.setPlainText("选择接口后可查看说明、标签和 schema。")

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
        content_layout.setSpacing(12)
        content_layout.addWidget(self.action_title)
        content_layout.addWidget(self.action_summary)
        content_layout.addWidget(self.action_tags)
        content_layout.addWidget(info_header)
        content_layout.addWidget(self.detail_pivot)
        content_layout.addWidget(self.detail_stack, 1)

        self.empty_title = StrongBodyLabel("选择一个接口开始调试", self.detail_empty_page)
        self.empty_title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.empty_hint = CaptionLabel("左侧会展示当前 Bot 可用的 Action 接口、说明和标签。", self.detail_empty_page)
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
        self.show_detail_page("params")

    def show_detail_page(self, route_key: str) -> None:
        widget = self._page_map[route_key]
        self.detail_stack.setCurrentWidget(widget)
        self.detail_pivot.setCurrentItem(route_key)

    def set_empty_state(self, title: str, message: str) -> None:
        self.empty_title.setText(title)
        self.empty_hint.setText(message)
        self.detail_state_stack.setCurrentWidget(self.detail_empty_page)

    def set_enabled_state(self, enabled: bool) -> None:
        self.generate_button.setEnabled(enabled)
        self.send_button.setEnabled(enabled)
        self.params_editor.setEnabled(enabled)
        if enabled:
            self.detail_state_stack.setCurrentWidget(self.detail_content_page)
