# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CardWidget,
    FluentIcon,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    ToolButton,
)

from src.core.api_debug import ApiDebugExecutionResult
from src.ui.components.code_editor.editor import JsonEditor
from src.ui.components.code_editor.exhibit import CodeExibit
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
            payload: dict[str, object] = {
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


class ActionDebugCard(CardWidget):
    """调试参数与执行结果面板。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugDebugCard")
        self.setMinimumWidth(400)
        self.setMaximumWidth(460)

        self.debug_close_button = ToolButton(FluentIcon.CLOSE, self)
        self.generate_button = PushButton(FluentIcon.ROTATE, "生成预设参数", self)
        self.send_button = PrimaryPushButton(FluentIcon.SEND, "发送调试请求", self)
        self.params_label = StrongBodyLabel("调试参数", self)
        self.params_hint = CaptionLabel("优先使用 payloadExample，没有示例时按 schema 自动生成默认 JSON", self)
        self.params_hint.setWordWrap(True)
        self.params_editor = JsonEditor(self)
        self.params_editor.setMinimumHeight(240)
        self.result_card = ActionResultCard(self)

        self._setup_layout()

    def _setup_layout(self) -> None:
        debug_header = QWidget(self)
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

        layout = QVBoxLayout(self)
        layout.setContentsMargins(18, 18, 18, 18)
        layout.setSpacing(12)
        layout.addWidget(debug_header)
        layout.addWidget(self.params_label)
        layout.addWidget(self.params_hint)
        layout.addWidget(self.params_editor)
        layout.addLayout(debug_action_layout)
        layout.addSpacing(4)
        layout.addWidget(self.result_card, 1)

    def set_controls_enabled(self, enabled: bool) -> None:
        self.debug_close_button.setEnabled(enabled)
        self.generate_button.setEnabled(enabled)
        self.send_button.setEnabled(enabled)
        self.params_editor.setEnabled(enabled)
