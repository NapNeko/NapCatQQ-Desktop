# -*- coding: utf-8 -*-
from __future__ import annotations

"""API 调试页面共享控件。"""

# 标准库导入
import json
from dataclasses import dataclass
from typing import Any, Callable

# 第三方库导入
from PySide6.QtCore import QObject, QRunnable, Qt, Signal
from PySide6.QtWidgets import (
    QGridLayout,
    QHBoxLayout,
    QListWidgetItem,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    CaptionLabel,
    CardWidget,
    CheckBox,
    ComboBox,
    LineEdit,
    ListWidget,
    MessageBoxBase,
    PillPushButton,
    PushButton,
    SearchLineEdit,
    SegmentedWidget,
    StrongBodyLabel,
    TitleLabel,
)

# 项目内模块导入
from src.core.api_debug.models import (
    ApiDebugAuthConfig,
    ApiDebugAuthType,
    ApiDebugErrorKind,
    ApiDebugExecutionResult,
    ApiDebugResponseBodyType,
    ApiDebugSearchItem,
)
from src.ui.components.code_editor.exhibit import CodeExibit
from src.ui.components.stacked_widget import TransparentStackedWidget


def pretty_json(payload: Any) -> str:
    """安全格式化 JSON。"""
    try:
        return json.dumps(payload, ensure_ascii=False, indent=2)
    except TypeError:
        return str(payload)


def mapping_from_json_text(text: str) -> dict[str, str]:
    """将 JSON 文本解析为字符串映射。"""
    raw = text.strip()
    if not raw:
        return {}
    parsed = json.loads(raw)
    if not isinstance(parsed, dict):
        raise ValueError("JSON 必须是对象")
    return {str(key): "" if value is None else str(value) for key, value in parsed.items()}


def safe_mapping_from_json_text(text: str) -> dict[str, str]:
    """在编辑过程中的宽松 JSON 映射解析。"""
    try:
        return mapping_from_json_text(text)
    except (TypeError, ValueError, json.JSONDecodeError):
        return {}


def find_index_by_data(combo: ComboBox, key: str) -> int:
    """根据 userData 查找下标。"""
    for index in range(combo.count()):
        if str(combo.itemData(index) or "") == key:
            return index
    return -1


@dataclass(slots=True)
class ResourceListItem:
    key: str
    title: str
    subtitle: str
    payload: Any


class CallableTask(QObject, QRunnable):
    """在 QThreadPool 中执行同步任务。"""

    result_ready = Signal(object)
    error_raised = Signal(str)
    finished = Signal()

    def __init__(self, func: Callable[[], Any]) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.func = func
        self.setAutoDelete(True)

    def run(self) -> None:
        try:
            self.result_ready.emit(self.func())
        except Exception as error:  # pragma: no cover - UI 侧统一处理
            self.error_raised.emit(str(error))
        finally:
            self.finished.emit()


class ApiDebugChip(PillPushButton):
    """顶部摘要胶囊。"""

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("ApiDebugChip")
        self.label = self
        self.setCheckable(False)
        self.setFixedHeight(28)
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.setText(text)

    def set_text(self, text: str) -> None:
        self.setText(text)


class ApiDebugSearchDialog(MessageBoxBase):
    """Action 搜索对话框。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.items: list[ApiDebugSearchItem] = []
        self.selected_item: ApiDebugSearchItem | None = None

        self.title_label = TitleLabel("搜索接口", self)
        self.search_edit = SearchLineEdit(self)
        self.search_edit.setPlaceholderText("搜索 Action 名称或简要说明")
        self.result_list = ListWidget(self)
        self.result_list.setUniformItemSizes(False)
        self.widget.setMinimumSize(620, 520)
        self.yesButton.setText("定位接口")
        self.cancelButton.setText("关闭")
        self.yesButton.setEnabled(False)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.search_edit)
        self.viewLayout.addWidget(self.result_list, 1)

        self.search_edit.textChanged.connect(self._rebuild_list)
        self.result_list.itemActivated.connect(self._accept_item)
        self.result_list.itemDoubleClicked.connect(self._accept_item)
        self.result_list.currentItemChanged.connect(self._handle_current_item_changed)
        self.yesButton.clicked.connect(self._accept_current_item)

    def set_items(self, items: list[ApiDebugSearchItem]) -> None:
        self.items = items
        self._rebuild_list()

    def open_and_choose(self, keyword: str = "") -> ApiDebugSearchItem | None:
        self.selected_item = None
        self.search_edit.setText(keyword)
        self.search_edit.selectAll()
        self.search_edit.setFocus()
        if self.exec():
            return self.selected_item
        return None

    def _rebuild_list(self) -> None:
        keyword = self.search_edit.text().strip().lower()
        self.result_list.clear()
        filtered = [
            item
            for item in self.items
            if not keyword
            or keyword in item.title.lower()
            or keyword in item.subtitle.lower()
            or keyword in item.mode.value
        ]

        for item in filtered[:200]:
            widget_item = QListWidgetItem(f"{item.title}\n{item.subtitle}")
            widget_item.setData(Qt.ItemDataRole.UserRole, item)
            self.result_list.addItem(widget_item)

        if self.result_list.count():
            self.result_list.setCurrentRow(0)
        else:
            self.selected_item = None
            self.yesButton.setEnabled(False)

    def _accept_item(self, item: QListWidgetItem) -> None:
        payload = item.data(Qt.ItemDataRole.UserRole)
        if isinstance(payload, ApiDebugSearchItem):
            self.selected_item = payload
            self.accept()

    def _handle_current_item_changed(self, current: QListWidgetItem | None, _previous: QListWidgetItem | None) -> None:
        payload = current.data(Qt.ItemDataRole.UserRole) if current is not None else None
        self.selected_item = payload if isinstance(payload, ApiDebugSearchItem) else None
        self.yesButton.setEnabled(self.selected_item is not None)

    def _accept_current_item(self) -> None:
        if self.selected_item is not None:
            self.accept()


class ResourcePanelWidget(CardWidget):
    """左侧资源栏。"""

    item_activated = Signal(str, object)

    def __init__(self, title: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.sections: dict[str, list[ResourceListItem]] = {}
        self.current_section = "catalog"

        self.title_label = StrongBodyLabel(title, self)
        self.search_edit = SearchLineEdit(self)
        self.search_edit.setPlaceholderText("搜索当前资源")
        self.pivot = SegmentedWidget(self)
        self.list_widget = ListWidget(self)
        self.content_stack = TransparentStackedWidget(self)
        self.footer_label = CaptionLabel("", self)
        self.list_widget.setUniformItemSizes(False)
        self.empty_state = QWidget(self)
        empty_layout = QVBoxLayout(self.empty_state)
        empty_layout.setContentsMargins(12, 24, 12, 24)
        empty_layout.setSpacing(6)
        empty_layout.addStretch(1)
        empty_layout.addWidget(StrongBodyLabel("暂无可用资源", self.empty_state), 0, Qt.AlignmentFlag.AlignHCenter)
        empty_layout.addWidget(
            CaptionLabel("从接口目录、历史记录或收藏预设开始", self.empty_state),
            0,
            Qt.AlignmentFlag.AlignHCenter,
        )
        empty_layout.addStretch(1)
        self.content_stack.addWidget(self.list_widget)
        self.content_stack.addWidget(self.empty_state)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(10)
        layout.addWidget(self.title_label)
        layout.addWidget(self.search_edit)
        layout.addWidget(self.pivot)
        layout.addWidget(self.content_stack, 1)
        layout.addWidget(self.footer_label)

        self.pivot.addItem("catalog", "接口目录", lambda: self.set_section("catalog"))
        self.pivot.addItem("history", "历史记录", lambda: self.set_section("history"))
        self.pivot.addItem("preset", "收藏预设", lambda: self.set_section("preset"))
        self.pivot.setCurrentItem("catalog")

        self.search_edit.textChanged.connect(self._refresh_list)
        self.list_widget.itemDoubleClicked.connect(self._emit_item)

    def set_sections(
        self,
        *,
        catalog: list[ResourceListItem],
        history: list[ResourceListItem],
        preset: list[ResourceListItem],
    ) -> None:
        self.sections = {"catalog": catalog, "history": history, "preset": preset}
        self._refresh_list()

    def set_section(self, key: str) -> None:
        self.current_section = key
        self.pivot.setCurrentItem(key)
        self._refresh_list()

    def _refresh_list(self) -> None:
        keyword = self.search_edit.text().strip().lower()
        items = self.sections.get(self.current_section, [])
        if keyword:
            items = [item for item in items if keyword in item.title.lower() or keyword in item.subtitle.lower()]

        self.list_widget.clear()
        for item in items[:300]:
            widget_item = QListWidgetItem(f"{item.title}\n{item.subtitle}")
            widget_item.setData(Qt.ItemDataRole.UserRole, item)
            self.list_widget.addItem(widget_item)

        self.content_stack.setCurrentWidget(self.list_widget if self.list_widget.count() else self.empty_state)
        item_count = self.list_widget.count()
        self.footer_label.setVisible(item_count > 0)
        self.footer_label.setText(f"共 {item_count} 项")

    def _emit_item(self, item: QListWidgetItem) -> None:
        payload = item.data(Qt.ItemDataRole.UserRole)
        if isinstance(payload, ResourceListItem):
            self.item_activated.emit(self.current_section, payload.payload)


class AuthEditorWidget(QWidget):
    """认证配置编辑器。"""

    auth_changed = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.mode_combo = ComboBox(self)
        self.mode_combo.addItem("无认证", userData=ApiDebugAuthType.NONE.value)
        self.mode_combo.addItem("Bearer Token", userData=ApiDebugAuthType.BEARER_TOKEN.value)
        self.mode_combo.addItem("WebUI Session", userData=ApiDebugAuthType.WEBUI_SESSION.value)
        self.mode_combo.setMinimumWidth(180)
        self.mode_label = CaptionLabel("认证模式", self)
        self.token_label = CaptionLabel("Token", self)
        self.credential_label = CaptionLabel("Credential", self)
        self.token_edit = LineEdit(self)
        self.token_edit.setPlaceholderText("Token / WebUI Token")
        self.credential_edit = LineEdit(self)
        self.credential_edit.setPlaceholderText("Session Credential")
        self.query_checkbox = CheckBox("使用 Query Token", self)

        layout = QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setHorizontalSpacing(12)
        layout.setVerticalSpacing(10)
        layout.addWidget(self.mode_label, 0, 0)
        layout.addWidget(self.mode_combo, 0, 1)
        layout.addWidget(self.token_label, 1, 0)
        layout.addWidget(self.token_edit, 1, 1)
        layout.addWidget(self.credential_label, 2, 0)
        layout.addWidget(self.credential_edit, 2, 1)
        layout.addWidget(self.query_checkbox, 3, 1)

        self.mode_combo.currentIndexChanged.connect(self._sync_visibility)
        self.mode_combo.currentIndexChanged.connect(lambda *_: self.auth_changed.emit())
        self.token_edit.textChanged.connect(lambda *_: self.auth_changed.emit())
        self.credential_edit.textChanged.connect(lambda *_: self.auth_changed.emit())
        self.query_checkbox.toggled.connect(lambda *_: self.auth_changed.emit())
        self.query_checkbox.toggled.connect(lambda *_: self._sync_visibility())
        self._sync_visibility()

    def set_auth(self, auth: ApiDebugAuthConfig) -> None:
        index = find_index_by_data(self.mode_combo, auth.auth_type.value)
        if index >= 0:
            self.mode_combo.setCurrentIndex(index)
        self.token_edit.setText(auth.token)
        self.credential_edit.setText(auth.session_credential)
        self.query_checkbox.setChecked(auth.use_query_token)
        self._sync_visibility()

    def get_auth(self) -> ApiDebugAuthConfig:
        raw_type = str(self.mode_combo.currentData() or ApiDebugAuthType.NONE.value)
        try:
            auth_type = ApiDebugAuthType(raw_type)
        except ValueError:
            auth_type = ApiDebugAuthType.NONE
        return ApiDebugAuthConfig(
            auth_type=auth_type,
            token=self.token_edit.text().strip(),
            session_credential=self.credential_edit.text().strip(),
            use_query_token=self.query_checkbox.isChecked(),
        )

    def _sync_visibility(self) -> None:
        raw_type = str(self.mode_combo.currentData() or ApiDebugAuthType.NONE.value)
        show_token = raw_type != ApiDebugAuthType.NONE.value
        show_credential = raw_type == ApiDebugAuthType.WEBUI_SESSION.value and not self.query_checkbox.isChecked()
        self.token_label.setVisible(show_token)
        self.token_edit.setVisible(show_token)
        self.credential_label.setVisible(show_credential)
        self.credential_edit.setVisible(show_credential)
        self.query_checkbox.setVisible(show_token)


class ResponseInspector(CardWidget):
    """右侧响应检查器。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.title_label = StrongBodyLabel("响应详情", self)
        self.status_chip = ApiDebugChip("未执行", self)
        self.meta_label = CaptionLabel("发送或执行后在这里查看正文、头信息和错误详情", self)
        self.meta_label.setWordWrap(True)
        self.pivot = SegmentedWidget(self)
        self.content_stack = TransparentStackedWidget(self)
        self.stack = TransparentStackedWidget(self)
        self.body_view = CodeExibit(self)
        self.headers_view = CodeExibit(self)
        self.error_view = CodeExibit(self)
        self.empty_page = QWidget(self)
        empty_layout = QVBoxLayout(self.empty_page)
        empty_layout.setContentsMargins(12, 24, 12, 24)
        empty_layout.setSpacing(6)
        empty_layout.addStretch(1)
        empty_layout.addWidget(StrongBodyLabel("等待一次执行", self.empty_page), 0, Qt.AlignmentFlag.AlignHCenter)
        empty_layout.addWidget(
            CaptionLabel("执行后会在这里展示正文、头信息与错误详情", self.empty_page),
            0,
            Qt.AlignmentFlag.AlignHCenter,
        )
        empty_layout.addStretch(1)

        self.stack.addWidget(self.body_view)
        self.stack.addWidget(self.headers_view)
        self.stack.addWidget(self.error_view)
        self.content_stack.addWidget(self.empty_page)
        self.content_stack.addWidget(self.stack)
        self.pivot.addItem("body", "正文", lambda: self._show("body", self.body_view))
        self.pivot.addItem("headers", "头信息", lambda: self._show("headers", self.headers_view))
        self.pivot.addItem("error", "错误", lambda: self._show("error", self.error_view))
        self._show("body", self.body_view)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(10)
        header_row = QWidget(self)
        header_layout = QHBoxLayout(header_row)
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)
        header_layout.addWidget(self.title_label)
        header_layout.addStretch(1)
        header_layout.addWidget(self.status_chip)
        layout.addWidget(header_row)
        layout.addWidget(self.meta_label)
        layout.addWidget(self.pivot)
        layout.addWidget(self.content_stack, 1)
        self.pivot.hide()
        self.content_stack.setCurrentWidget(self.empty_page)
        self._set_content_placeholders()

    def apply_result(self, result: ApiDebugExecutionResult | None) -> None:
        if result is None:
            self.status_chip.set_text("未执行")
            self.meta_label.setText("发送或执行后在这里查看正文、头信息和错误详情")
            self._set_content_placeholders()
            self.pivot.setVisible(False)
            self.content_stack.setCurrentWidget(self.empty_page)
            return

        self.pivot.setVisible(True)
        self.content_stack.setCurrentWidget(self.stack)
        if result.response is not None:
            self.status_chip.set_text(str(result.response.status_code))
            self.meta_label.setText(
                " · ".join(
                    part
                    for part in (
                        result.response.reason_phrase or "",
                        f"{result.response.elapsed_ms:.2f} ms",
                        self._format_size(result.response.size_bytes),
                        self._body_type_label(result.response.body_type),
                    )
                    if part
                )
            )
            self.body_view.setPlainText(result.response.formatted_body or "响应体为空")
            self.headers_view.setPlainText(pretty_json(result.response.headers) if result.response.headers else "{}")
            if result.error is None:
                self._show("body", self.body_view)
        else:
            self.status_chip.set_text("失败")
            self.meta_label.setText("没有收到可展示的响应")
            self.body_view.setPlainText("本次执行没有返回响应体")
            self.headers_view.setPlainText("{}")

        if result.error is not None:
            error_payload = {
                "kind": self._error_kind_label(result.error.kind),
                "message": result.error.message,
            }
            if result.error.status_code is not None:
                error_payload["status_code"] = result.error.status_code
            if result.error.details:
                error_payload["details"] = result.error.details
            self.error_view.setPlainText(pretty_json(error_payload))
            self._show("error", self.error_view)
        else:
            self.error_view.setPlainText("本次执行没有错误信息")

    def _show(self, route_key: str, widget: QWidget) -> None:
        self.stack.setCurrentWidget(widget)
        self.pivot.setCurrentItem(route_key)

    def _set_content_placeholders(self) -> None:
        self.body_view.setPlainText("响应体会显示在这里")
        self.headers_view.setPlainText("{}")
        self.error_view.setPlainText("没有错误信息")

    @staticmethod
    def _body_type_label(body_type: ApiDebugResponseBodyType) -> str:
        mapping = {
            ApiDebugResponseBodyType.EMPTY: "空响应",
            ApiDebugResponseBodyType.JSON: "JSON",
            ApiDebugResponseBodyType.TEXT: "文本",
            ApiDebugResponseBodyType.BINARY: "二进制",
        }
        return mapping.get(body_type, body_type.value)

    @staticmethod
    def _error_kind_label(kind: ApiDebugErrorKind) -> str:
        mapping = {
            ApiDebugErrorKind.REQUEST_BUILD: "请求构造错误",
            ApiDebugErrorKind.AUTH: "认证错误",
            ApiDebugErrorKind.NETWORK: "网络错误",
            ApiDebugErrorKind.TIMEOUT: "请求超时",
            ApiDebugErrorKind.HTTP_STATUS: "HTTP 状态错误",
            ApiDebugErrorKind.HISTORY: "历史记录错误",
            ApiDebugErrorKind.UNKNOWN: "未知错误",
        }
        return mapping.get(kind, kind.value)

    @staticmethod
    def _format_size(size_bytes: int) -> str:
        if size_bytes < 1024:
            return f"{size_bytes} B"
        if size_bytes < 1024 * 1024:
            return f"{size_bytes / 1024:.1f} KB"
        return f"{size_bytes / (1024 * 1024):.1f} MB"
