# -*- coding: utf-8 -*-

# 标准库导入
import json
import os
from types import SimpleNamespace
from urllib.parse import parse_qs, urlsplit

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, QPoint, QSize, Qt, Signal
from PySide6.QtNetwork import QAbstractSocket, QNetworkRequest
from PySide6.QtWidgets import QApplication, QSizePolicy, QVBoxLayout, QWidget

# 项目内模块导入
from src.desktop.core.api_debug import (
    ApiDebugAuthConfig,
    ApiDebugBotContext,
    ApiDebugContextService,
    ApiDebugEndpointSummary,
    ApiDebugMode,
    ApiDebugTargetType,
    ApiDebugWebSocketDirection,
    ApiDebugWebSocketService,
    ApiDebugWorkspaceState,
    ApiDebugWorkspaceStore,
    SchemaDefaultGenerator,
)
from src.desktop.core.api_debug.models import (
    ApiDebugActionSchema,
    ApiDebugBodyType,
    ApiDebugBuiltRequest,
    ApiDebugError,
    ApiDebugErrorKind,
    ApiDebugExecutionResult,
    ApiDebugHttpDraft,
    ApiDebugResponse,
    ApiDebugResponseBodyType,
)
from src.desktop.core.config.config_model import HttpServersConfig, WebsocketServersConfig
import src.desktop.core.api_debug.workspace_store as workspace_store_module
import src.desktop.ui.page.api_debug_page as api_debug_page_module
from src.desktop.ui.window.main_window import window as main_window_module


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


class FakeLoginStateManager:
    def __init__(self, login_state) -> None:
        self.login_state = login_state

    def get_login_state(self, qq_id: str):
        return self.login_state if qq_id == "114514" else None


class FakeSocket(QObject):
    connected = Signal()
    disconnected = Signal()
    textMessageReceived = Signal(str)
    binaryMessageReceived = Signal(bytes)
    stateChanged = Signal(int)
    errorOccurred = Signal(object)

    def __init__(self) -> None:
        super().__init__()
        self.last_request: QNetworkRequest | None = None
        self.sent_messages: list[str] = []

    def open(self, request: QNetworkRequest) -> None:
        self.last_request = request
        self.stateChanged.emit(QAbstractSocket.SocketState.ConnectingState.value)

    def close(self) -> None:
        self.stateChanged.emit(QAbstractSocket.SocketState.UnconnectedState.value)
        self.disconnected.emit()

    def deleteLater(self) -> None:
        return None

    def sendTextMessage(self, text: str) -> None:
        self.sent_messages.append(text)

    def errorString(self) -> str:
        return "socket-error"


def test_schema_default_generator_matches_webui_priority() -> None:
    generator = SchemaDefaultGenerator()
    schema = {
        "type": "object",
        "properties": {
            "fixed": {"const": "napcat"},
            "level": {"default": 2},
            "mode": {"enum": ["http", "ws"]},
            "detail": {"oneOf": [{"type": "string"}, {"type": "object", "properties": {"ok": {"type": "boolean"}}}]},
            "items": {"type": "array"},
        },
    }

    payload = generator.build_default(schema)

    assert payload == {
        "fixed": "napcat",
        "level": 2,
        "mode": "http",
        "detail": "",
        "items": [],
    }
    assert generator.build_default(schema, {"custom": True}) == {"custom": True}


def test_context_service_builds_http_webui_and_ws_targets(config_factory) -> None:
    config = config_factory()
    config.connect.httpServers.append(
        HttpServersConfig(
            enable=True,
            name="main-http",
            messagePostFormat="array",
            token="onebot-token",
            debug=False,
            host="0.0.0.0",
            port=3000,
            enableCors=False,
            enableWebsocket=True,
        )
    )
    config.connect.websocketServers.append(
        WebsocketServersConfig(
            enable=True,
            name="main-ws",
            messagePostFormat="array",
            token="ws-token",
            debug=False,
            host="::",
            port=3001,
            reportSelfMessage=False,
            enableForcePushEvent=False,
            heartInterval=30000,
        )
    )

    login_state = SimpleNamespace(port=6099, token="webui-token", auth="credential")
    service = ApiDebugContextService(
        config_reader=lambda: [config],
        login_state_provider=FakeLoginStateManager(login_state),
    )

    contexts = service.list_bot_contexts()
    assert len(contexts) == 1
    context = contexts[0]
    assert context.preferred_http_target() is not None
    assert context.preferred_http_target().url == "http://127.0.0.1:3000"
    assert context.preferred_http_target().auth_config == ApiDebugAuthConfig.bearer("onebot-token")
    assert context.preferred_action_base_url() == "http://127.0.0.1:6099"
    assert any(target.target_type == ApiDebugTargetType.DEBUG_WEBSOCKET for target in context.websocket_targets)
    assert any(target.url == "ws://127.0.0.1:3001" for target in context.websocket_targets)


def test_workspace_store_redacts_sensitive_values(tmp_path) -> None:
    store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    state = ApiDebugWorkspaceState(
        selected_bot_id="114514",
        selected_mode=ApiDebugMode.HTTP,
        http_draft=ApiDebugHttpDraft(
            url="http://127.0.0.1:3000/api/test?access_token=secret",
            method="POST",
            headers={"Authorization": "Bearer secret"},
            query_params={"access_token": "secret"},
            body_type=ApiDebugBodyType.JSON,
            body={"token": "secret", "nested": {"cookie": "cookie"}},
            auth=ApiDebugAuthConfig.bearer("top-secret"),
        ),
    )

    store.save(state)
    raw_payload = json.loads((tmp_path / "workspace.json").read_text(encoding="utf-8"))
    assert raw_payload["http_draft"]["auth"]["token"] == "<redacted>"
    assert raw_payload["http_draft"]["headers"]["Authorization"] == "<redacted>"
    assert raw_payload["http_draft"]["query_params"]["access_token"] == "<redacted>"
    assert raw_payload["http_draft"]["body"]["token"] == "<redacted>"


def test_workspace_store_retries_replace_when_file_is_temporarily_locked(tmp_path, monkeypatch) -> None:
    store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    state = ApiDebugWorkspaceState(selected_bot_id="114514")
    attempts = {"count": 0}
    original_replace = workspace_store_module.os.replace

    def flaky_replace(src, dst):
        attempts["count"] += 1
        if attempts["count"] < 3:
            raise PermissionError(5, "拒绝访问。", str(dst))
        return original_replace(src, dst)

    monkeypatch.setattr(workspace_store_module.os, "replace", flaky_replace)

    store.save(state)

    assert attempts["count"] == 3
    assert json.loads((tmp_path / "workspace.json").read_text(encoding="utf-8"))["selected_bot_id"] == "114514"


def test_websocket_service_injects_query_token_and_logs_messages() -> None:
    ensure_qapp()
    fake_socket = FakeSocket()
    service = ApiDebugWebSocketService(socket_factory=lambda: fake_socket)
    messages = []
    service.message_logged.connect(messages.append)

    service.connect_onebot("ws://127.0.0.1:3001/api", ApiDebugAuthConfig.bearer("onebot-token", use_query_token=True))

    assert fake_socket.last_request is not None
    query = parse_qs(urlsplit(fake_socket.last_request.url().toString()).query)
    assert query["access_token"] == ["onebot-token"]

    fake_socket.connected.emit()
    fake_socket.textMessageReceived.emit('{"ok": true}')
    service.send_text('{"echo": true}')

    assert any(item.direction == ApiDebugWebSocketDirection.SYSTEM for item in messages)
    assert any(item.direction == ApiDebugWebSocketDirection.INCOMING for item in messages)
    assert any(item.direction == ApiDebugWebSocketDirection.OUTGOING for item in messages)
    assert fake_socket.sent_messages == ['{"echo": true}']


def test_interface_debug_page_loads_schema_and_restores_draft(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="send_msg",
        summary="发送消息",
        description="用于发送消息",
        payload_schema={"type": "object", "properties": {"message": {"type": "string"}}},
        return_schema={"type": "object", "properties": {"message_id": {"type": "integer"}}},
        action_tags=["msg"],
    )
    context = ApiDebugBotContext(
        bot_id="114514",
        bot_name="TestBot",
        webui_base_url="http://127.0.0.1:6099",
        webui_token="token",
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.workspace_state.selected_bot_id = "114514"
    page.workspace_state.action_draft.action = "send_msg"
    page.workspace_state.action_draft.search_query = "send"
    page.workspace_state.action_draft.params_text = '{\n  "message": "hello"\n}'
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [context])
    page._run_async = lambda func, **kwargs: kwargs["on_success"](func())
    page.action_service.fetch_schemas = lambda *_args, **_kwargs: [schema]

    host = QWidget()
    page.initialize(host)

    assert not hasattr(page, "top_bar")
    assert page.catalog_panel.bot_combo.currentText() == "TestBot (114514)"
    assert page.catalog_panel.bot_combo.minimumWidth() == 180
    assert not hasattr(page, "mode_pivot")
    assert not hasattr(page, "search_dialog")
    assert page.catalog_panel.search_edit.toolTip() == "打开接口搜索对话框"
    assert page.catalog_panel.search_edit.isReadOnly() is True
    assert page.category_label.text() == "msg"
    assert page.action_title.text() == "发送消息"
    assert page.request_method_chip.text() == "POST"
    assert page.request_route_label.text() == "/send_msg"
    assert page.detail_panel.pinned_meta_card.isHidden() is True
    assert page.docs_scroll.verticalScrollBarPolicy() == Qt.ScrollBarPolicy.ScrollBarAlwaysOff
    assert page.catalog_panel.footer_label.isHidden() is True
    assert page.detail_panel.body_params_title.text() == "Body 参数"
    assert page.detail_panel.body_content_type_chip.text() == "application/json"
    assert page.detail_panel.body_header_divider.height() == 1
    assert page.detail_panel.example_toggle_button.text() == "示例  ▼"
    assert page.detail_panel.params_rows_layout.count() == 1
    assert "message_id" in page.response_view.toPlainText()
    assert page.debug_card.isHidden() is True
    assert json.loads(page.params_editor.toPlainText()) == {"message": "hello"}

    page.close()
    host.close()


def test_interface_debug_page_generates_default_payload_for_new_selection(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="get_friend_list",
        summary="好友列表",
        payload_schema={"type": "object", "properties": {"refresh": {"type": "boolean"}}},
        action_tags=["user"],
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page._apply_schema(schema)

    assert page.action_title.text() == "好友列表"
    assert page.request_route_label.text() == "/get_friend_list"
    assert json.loads(page.params_editor.toPlainText()) == {"refresh": False}
    assert page.detail_panel.params_rows_layout.count() == 1

    page.close()
    host.close()


def test_interface_debug_page_persist_workspace_does_not_raise_on_save_error(tmp_path, monkeypatch) -> None:
    ensure_qapp()

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)

    monkeypatch.setattr(
        page.workspace_store,
        "save",
        lambda _state: (_ for _ in ()).throw(PermissionError(5, "拒绝访问。", "workspace.json")),
    )

    page._persist_workspace()

    page.close()
    host.close()


def test_interface_debug_page_catalog_groups_actions_as_tree(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="delete_group_album_media",
        summary="删除群相册媒体",
        action_tags=["group"],
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page.catalog_panel.set_schemas([schema], "delete_group_album_media")

    category_item = page.catalog_panel.tree_widget.topLevelItem(0)
    action_item = category_item.child(0)

    assert page.catalog_panel.tree_widget.topLevelItemCount() == 1
    assert category_item.text(0) == "group"
    assert category_item.isExpanded() is False
    assert action_item.data(0, Qt.ItemDataRole.UserRole) == "delete_group_album_media"
    assert action_item.text(0) == "删除群相册媒体"

    page.close()
    host.close()


def test_interface_debug_page_catalog_selects_tree_leaf_item(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="delete_group_album_media",
        summary="删除群相册媒体",
        action_tags=["group"],
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page.catalog_panel.set_schemas([schema], "delete_group_album_media")

    current_item = page.catalog_panel.tree_widget.currentItem()

    assert current_item is not None
    assert current_item.text(0) == "删除群相册媒体"
    assert current_item.data(0, Qt.ItemDataRole.UserRole) == "delete_group_album_media"

    page.close()
    host.close()


def test_interface_debug_page_debug_panel_hidden_until_requested(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(action="send_msg", summary="发送消息", action_tags=["msg"])

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page._apply_schema(schema)

    assert page.debug_card.isHidden() is True

    page.debug_button.click()
    QApplication.processEvents()

    assert page.debug_card.isHidden() is False

    page.close()
    host.close()


def test_interface_debug_page_debug_panel_pushes_docs_area_when_opened(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="send_msg",
        summary="发送消息",
        payload_schema={"type": "object", "properties": {"message": {"type": "string"}}},
    )

    host = QWidget()
    host.resize(1500, 900)
    host_layout = QVBoxLayout(host)
    host_layout.setContentsMargins(0, 0, 0, 0)

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    page.initialize(host)
    host_layout.addWidget(page)
    page._apply_schema(schema)
    host.show()
    QApplication.processEvents()

    before_docs_width = page.docs_scroll.width()
    before_panel_width = page.detail_panel.debug_panel_container.width()

    page.debug_button.click()
    QApplication.processEvents()

    after_docs_width = page.docs_scroll.width()
    after_panel_width = page.detail_panel.debug_panel_container.width()
    docs_top_left = page.docs_scroll.mapTo(page.detail_panel.detail_content_page, QPoint(0, 0))
    debug_top_left = page.detail_panel.debug_panel_container.mapTo(page.detail_panel.detail_content_page, QPoint(0, 0))

    assert before_panel_width == 0
    assert after_panel_width >= 400
    assert after_docs_width < before_docs_width
    assert docs_top_left.x() + page.docs_scroll.width() <= debug_top_left.x()

    page.close()
    host.close()


def test_interface_debug_page_debug_panel_uses_bounded_editor_height(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(
        action="send_msg",
        summary="发送消息",
        payload_schema={"type": "object", "properties": {"message": {"type": "string"}}},
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page._apply_schema(schema)

    assert page.params_editor.minimumHeight() == 160
    assert page.params_editor.maximumHeight() == 280
    assert page.debug_card.section_splitter.orientation() == Qt.Orientation.Vertical
    assert page.debug_card.section_splitter.handleWidth() == 10
    assert page.debug_card.section_splitter.childrenCollapsible() is True
    assert page.params_label.text() == "Body"
    assert page.result_card.title_label.text() == "返回结果"

    page.debug_button.click()
    QApplication.processEvents()

    assert page.result_card.content_view.minimumHeight() == 160
    assert page.params_editor.height() <= 280

    page.close()
    host.close()


def test_interface_debug_page_debug_panel_uses_floating_workbench_style(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(action="send_msg", summary="发送消息")

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page._apply_schema(schema)

    debug_container_margins = page.detail_panel.debug_panel_container.layout().contentsMargins()

    assert debug_container_margins.top() == 8
    assert debug_container_margins.bottom() == 12
    assert page.debug_card.top_section.objectName() == "ApiDebugParamsSection"
    assert page.debug_card.runtime_title.text() == "在线运行"
    assert page.debug_card.runtime_meta_card.objectName() == "ApiDebugRuntimeMetaCard"
    assert page.generate_button.isHidden() is True
    assert page.send_button.minimumHeight() == 28
    assert page.debug_card.runtime_meta_card.minimumHeight() == 28
    assert page.debug_card.action_bar.layout().stretch(0) == 3
    assert page.debug_card.action_bar.layout().stretch(1) == 1
    assert page.send_button.text() == "发送"
    assert page.runtime_route_label.text() == "/send_msg"


def test_interface_debug_page_splitter_can_fold_to_section_hints(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(action="send_msg", summary="发送消息")

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [])
    host = QWidget()
    page.initialize(host)
    page._apply_schema(schema)
    page.debug_button.click()
    QApplication.processEvents()

    page.debug_card.section_splitter.setSizes([0, 600])
    QApplication.processEvents()
    assert page.debug_card.top_content_container.isHidden() is True
    assert page.debug_card.runtime_title.text() == "在线运行"

    page.debug_card.section_splitter.setSizes([600, 0])
    QApplication.processEvents()
    assert page.result_card.content_container.isHidden() is True
    assert page.result_card.title_label.text() == "返回结果"

    page.close()
    host.close()

    page.close()
    host.close()


def test_interface_debug_page_send_request_shows_result(tmp_path) -> None:
    ensure_qapp()
    schema = ApiDebugActionSchema(action="send_msg", summary="发送消息", payload_schema={"type": "object"})
    context = ApiDebugBotContext(
        bot_id="114514",
        bot_name="TestBot",
        webui_base_url="http://127.0.0.1:6099",
        webui_token="token",
    )
    result = ApiDebugExecutionResult(
        request=ApiDebugBuiltRequest(method="POST", url="http://127.0.0.1:6099/api/Debug/call/debug-primary"),
        response=ApiDebugResponse(
            status_code=200,
            reason_phrase="OK",
            headers={},
            body_type=ApiDebugResponseBodyType.JSON,
            formatted_body='{\n  "status": "ok"\n}',
            elapsed_ms=10.0,
            size_bytes=18,
        ),
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.current_context = context
    page.current_auth = ApiDebugAuthConfig.webui_session(token="token")
    page.current_session = None
    page.schemas = [schema]
    page._run_async = lambda func, **kwargs: kwargs["on_success"](func()) if "on_success" in kwargs else func()
    page.action_service.create_session = lambda *_args, **_kwargs: SimpleNamespace(
        base_url="http://127.0.0.1:6099", adapter_name="debug-primary"
    )
    page.action_service.call_action = lambda *_args, **_kwargs: result
    host = QWidget()
    page.initialize(host)
    page.schemas = [schema]
    page.current_context = context
    page.current_auth = ApiDebugAuthConfig.webui_session(token="token")
    page._apply_schema(schema)
    page.params_editor.setPlainText('{"message":"hi"}')

    page._send_current_action()

    assert page.debug_card.isHidden() is False
    assert page.result_card.status_chip.text() == "200"
    assert page.result_card.content_view.toPlainText() == '{\n  "status": "ok"\n}'

    page.close()
    host.close()


def test_interface_debug_page_shows_unavailable_state_without_webui(tmp_path) -> None:
    ensure_qapp()
    context = ApiDebugBotContext(
        bot_id="114514",
        bot_name="TestBot",
        http_targets=[],
        websocket_targets=[],
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [context])
    host = QWidget()
    page.initialize(host)

    assert page.empty_title.text() == "当前 Bot 暂不可调试"
    assert page.empty_container.minimumWidth() == 360
    assert page.empty_container.sizePolicy().verticalPolicy() == QSizePolicy.Policy.Maximum
    assert not page.send_button.isEnabled()
    assert not page.params_editor.isEnabled()


def test_interface_debug_page_uses_dedicated_loading_state(tmp_path) -> None:
    ensure_qapp()
    context = ApiDebugBotContext(
        bot_id="114514",
        bot_name="TestBot",
        webui_base_url="http://127.0.0.1:6099",
        webui_token="token",
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [context])
    page._run_async = lambda func, **kwargs: None
    host = QWidget()
    page.initialize(host)

    assert page.detail_state_stack.currentWidget() is page.detail_panel.detail_loading_page
    assert page.detail_panel.loading_title.text() == "正在加载接口"
    assert page.detail_panel.loading_skeleton.minimumHeight() == 220

    page.close()
    host.close()


def test_interface_debug_page_filters_internal_and_dot_actions(tmp_path) -> None:
    ensure_qapp()
    visible_schema = ApiDebugActionSchema(action="send_msg", summary="发送消息")
    dot_schema = ApiDebugActionSchema(action=".ocr_image", summary="图片 OCR 识别")
    internal_schema = ApiDebugActionSchema(action="debug_internal", summary="内部调试接口")
    context = ApiDebugBotContext(
        bot_id="114514",
        bot_name="TestBot",
        webui_base_url="http://127.0.0.1:6099",
        webui_token="token",
    )

    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.workspace_state.selected_bot_id = "114514"
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [context])
    page._run_async = lambda func, **kwargs: kwargs["on_success"](func())
    page.action_service.fetch_schemas = lambda *_args, **_kwargs: [visible_schema, dot_schema, internal_schema]

    host = QWidget()
    page.initialize(host)

    category_item = page.catalog_panel.tree_widget.topLevelItem(0)

    assert page.catalog_panel.tree_widget.topLevelItemCount() == 1
    assert category_item.childCount() == 1
    assert category_item.isExpanded() is False
    assert category_item.child(0).data(0, Qt.ItemDataRole.UserRole) == "send_msg"

    page.close()
    host.close()

    page.close()
    host.close()


def test_interface_debug_page_uses_bounded_expanding_content_layout(tmp_path) -> None:
    ensure_qapp()
    context = ApiDebugBotContext(bot_id="114514", bot_name="TestBot")

    host = QWidget()
    host.resize(1500, 900)
    host_layout = QVBoxLayout(host)
    host_layout.setContentsMargins(0, 0, 0, 0)
    page = api_debug_page_module.ApiDebugPage()
    page.workspace_store = ApiDebugWorkspaceStore(storage_path=tmp_path / "workspace.json")
    page.workspace_state = page.workspace_store.load()
    page.context_service = SimpleNamespace(list_bot_contexts=lambda: [context])
    page.initialize(host)
    host_layout.addWidget(page)
    host.show()
    QApplication.processEvents()

    assert page.content_widget.sizePolicy().horizontalPolicy() == QSizePolicy.Policy.MinimumExpanding
    assert page.content_widget.maximumWidth() == 1680
    assert page.detail_widget.minimumWidth() == 520
    assert page.minimumSizeHint() == api_debug_page_module.ApiDebugPage.STABLE_MINIMUM_SIZE_HINT
    assert page.sizeHint() == api_debug_page_module.ApiDebugPage.STABLE_SIZE_HINT
    assert page.hasHeightForWidth() is False
    assert page.heightForWidth(1320) == -1

    initial_size = host.size()
    host.move(host.x() + 20, host.y() + 20)
    QApplication.processEvents()
    assert host.size() == initial_size

    assert page.root_splitter.orientation() == Qt.Orientation.Horizontal
    assert page.root_splitter.sizes()[0] > 0
    assert page.root_splitter.orientation() == Qt.Orientation.Horizontal

    page.close()
    host.close()


def test_main_window_registers_interface_debug_between_bot_and_component(monkeypatch) -> None:
    ensure_qapp()

    class FakePage:
        def __init__(self, name: str) -> None:
            self.name = name

        def initialize(self, parent):
            return self

    recorded = []
    monkeypatch.setattr(main_window_module, "it", lambda cls: FakePage(cls.__name__))
    monkeypatch.setattr(
        main_window_module.MainWindow, "addSubInterface", lambda self, **kwargs: recorded.append(kwargs["text"])
    )

    window = main_window_module.MainWindow()
    window._set_item()

    assert recorded[:3] == ["主页", "BOT", "接口调试"]
    assert recorded[3:] == ["组件", "设置"]


def test_main_window_constructs() -> None:
    ensure_qapp()
    window = main_window_module.MainWindow()
    assert window is not None
    window.close()


def test_creart_supports_api_debug_page_creator() -> None:
    ensure_qapp()
    instance = it(api_debug_page_module.ApiDebugPage)
    assert isinstance(instance, api_debug_page_module.ApiDebugPage)
