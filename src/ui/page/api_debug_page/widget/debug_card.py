# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QSplitter, QVBoxLayout, QWidget
from qfluentwidgets import (
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
from ..common import pretty_json
from ..shared import ApiDebugChip
from .method_badge import MethodBadge, apply_method_badge


class ActionResultCard(QWidget):
    """执行结果区域。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugResultSection")
        self.setMinimumHeight(48)
        self.title_label = StrongBodyLabel("返回结果", self)
        self.status_chip = ApiDebugChip("未执行", self)
        self.meta_label = CaptionLabel("发送后会在这里展示本次接口返回的 JSON 预览", self)
        self.meta_label.setWordWrap(True)
        self.content_container = QWidget(self)
        self.content_container.setObjectName("ApiDebugResultContent")
        self.content_view = CodeExibit(self)
        self.content_view.setObjectName("ApiDebugResultEditor")
        self.content_view.setMinimumHeight(160)
        self.content_view.setPlainText("等待返回结果")
        self.content_view.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        header = QWidget(self)
        header_layout = QHBoxLayout(header)
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)
        header_layout.addWidget(self.title_label)
        header_layout.addStretch(1)
        header_layout.addWidget(self.status_chip)

        content_layout = QVBoxLayout(self.content_container)
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(10)
        content_layout.addWidget(self.meta_label)
        content_layout.addWidget(self.content_view, 1)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(10)
        layout.addWidget(header)
        layout.addWidget(self.content_container, 1)

    def reset(self, message: str = "等待一次执行") -> None:
        self.status_chip.set_text("未执行")
        self.meta_label.setText("发送后会在这里展示本次接口返回的 JSON 预览")
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

    def set_preview_visible(self, visible: bool) -> None:
        self.content_container.setVisible(visible)


class RuntimeEndpointCard(CardWidget):
    """右侧在线运行面板复用的接口摘要卡片。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugRuntimeMetaCard")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setMinimumHeight(32)
        self.setMaximumHeight(32)

        self.method_chip = MethodBadge("POST", self)
        self.route_label = StrongBodyLabel("/action_name", self)
        self.route_label.setObjectName("ApiDebugRouteLabel")
        self.route_label.setWordWrap(True)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(10, 8, 10, 8)
        layout.setSpacing(8)
        layout.addWidget(self.method_chip, 0, Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self.route_label, 1, Qt.AlignmentFlag.AlignVCenter)

    def set_method_style(self, method_text: str) -> None:
        apply_method_badge(self.method_chip, method_text)

    def set_route(self, route_text: str) -> None:
        self.route_label.setText(route_text)


class ActionDebugCard(CardWidget):
    """接口示例与调用结果面板。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("ApiDebugDebugCard")
        self.setMinimumWidth(400)
        self.setMaximumWidth(460)
        self._top_content_visible = True
        self._result_preview_visible = True

        self.debug_close_button = ToolButton(FluentIcon.CLOSE, self)
        self.generate_button = PushButton(FluentIcon.ROTATE, "生成预设参数", self)
        self.runtime_title = StrongBodyLabel("接口示例", self)
        self.runtime_meta_card = RuntimeEndpointCard(self)
        self.send_button = PrimaryPushButton(FluentIcon.SEND, "调用接口", self)
        self.params_label = StrongBodyLabel("Body", self)
        self.params_editor_shell = QWidget(self)
        self.params_editor_shell.setObjectName("ApiDebugEditorShell")
        self.params_editor = JsonEditor(self)
        self.params_editor.setObjectName("ApiDebugParamsEditor")
        self.params_editor.setMinimumHeight(160)
        self.params_editor.setMaximumHeight(280)
        self.params_editor.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.top_section = QWidget(self)
        self.top_section.setObjectName("ApiDebugParamsSection")
        self.top_section.setMinimumHeight(0)
        self.top_content_container = QWidget(self.top_section)
        self.top_content_container.setObjectName("ApiDebugTopContent")
        self.top_content_container.setMinimumHeight(0)
        self.result_card = ActionResultCard(self)
        self.result_card.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.result_card.content_container.setMinimumHeight(0)

        self._setup_layout()

    def _setup_layout(self) -> None:
        self.debug_header = QWidget(self)
        self.debug_header.setObjectName("ApiDebugWorkbenchHeader")
        debug_header_layout = QHBoxLayout(self.debug_header)
        debug_header_layout.setContentsMargins(0, 0, 0, 0)
        debug_header_layout.setSpacing(8)
        debug_header_layout.addWidget(self.runtime_title)
        debug_header_layout.addStretch(1)
        debug_header_layout.addWidget(self.debug_close_button)

        self.send_button.setMinimumHeight(36)
        self.send_button.setMaximumHeight(28)
        self.send_button.setMinimumHeight(28)
        self.send_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.generate_button.hide()

        self.action_bar = QWidget(self)
        self.action_bar.setObjectName("ApiDebugActionBar")
        action_layout = QHBoxLayout(self.action_bar)
        action_layout.setContentsMargins(0, 0, 0, 0)
        action_layout.setSpacing(10)
        action_layout.addWidget(self.runtime_meta_card, 3)
        action_layout.addWidget(self.send_button, 1)

        params_editor_layout = QVBoxLayout(self.params_editor_shell)
        params_editor_layout.setContentsMargins(1, 1, 1, 1)
        params_editor_layout.setSpacing(0)
        params_editor_layout.addWidget(self.params_editor)

        top_content_layout = QVBoxLayout(self.top_content_container)
        top_content_layout.setContentsMargins(0, 0, 0, 0)
        top_content_layout.setSpacing(12)
        top_content_layout.addWidget(self.action_bar)
        top_content_layout.addWidget(self.params_label)
        top_content_layout.addWidget(self.params_editor_shell, 1)

        top_section_layout = QVBoxLayout(self.top_section)
        top_section_layout.setContentsMargins(0, 0, 0, 0)
        top_section_layout.setSpacing(0)
        top_section_layout.addWidget(self.top_content_container, 1)

        self.section_splitter = QSplitter(Qt.Orientation.Vertical, self)
        self.section_splitter.setObjectName("ApiDebugVerticalSplitter")
        self.section_splitter.setChildrenCollapsible(True)
        self.section_splitter.setHandleWidth(10)
        self.section_splitter.addWidget(self.top_section)
        self.section_splitter.addWidget(self.result_card)
        self.section_splitter.setCollapsible(0, True)
        self.section_splitter.setCollapsible(1, True)
        self.section_splitter.setStretchFactor(0, 1)
        self.section_splitter.setStretchFactor(1, 1)
        self.section_splitter.setSizes([340, 360])
        self.section_splitter.splitterMoved.connect(self._sync_splitter_sections)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(14)
        layout.addWidget(self.debug_header)
        layout.addWidget(self.section_splitter, 1)

    def set_controls_enabled(self, enabled: bool) -> None:
        self.debug_close_button.setEnabled(enabled)
        self.generate_button.setEnabled(enabled)
        self.send_button.setEnabled(enabled)
        self.params_editor.setEnabled(enabled)
        self.runtime_meta_card.setEnabled(enabled)

    def showEvent(self, event) -> None:  # noqa: ANN001
        super().showEvent(event)
        self._sync_splitter_sections()

    def resizeEvent(self, event) -> None:  # noqa: ANN001
        super().resizeEvent(event)
        self._sync_splitter_sections()

    def _sync_splitter_sections(self, *_args) -> None:
        top_size, bottom_size = self.section_splitter.sizes()
        if top_size + bottom_size <= 0:
            self._set_top_content_visible(True)
            self._set_result_preview_visible(True)
            return

        top_hide_threshold = 88
        top_show_threshold = 132
        result_hide_threshold = 24
        result_show_threshold = 72

        if self._top_content_visible:
            if top_size <= top_hide_threshold:
                self._set_top_content_visible(False)
        elif top_size >= top_show_threshold:
            self._set_top_content_visible(True)

        if self._result_preview_visible:
            if bottom_size <= result_hide_threshold:
                self._set_result_preview_visible(False)
        elif bottom_size >= result_show_threshold:
            self._set_result_preview_visible(True)

    def _set_top_content_visible(self, visible: bool) -> None:
        if self._top_content_visible == visible:
            return
        self._top_content_visible = visible
        self.top_content_container.setVisible(visible)

    def _set_result_preview_visible(self, visible: bool) -> None:
        if self._result_preview_visible == visible:
            return
        self._result_preview_visible = visible
        self.result_card.set_preview_visible(visible)
