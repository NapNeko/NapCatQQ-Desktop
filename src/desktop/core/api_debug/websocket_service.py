# -*- coding: utf-8 -*-
"""接口调试 WebSocket 服务。"""

# 标准库导入
import json
import uuid
from typing import Any, Callable
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

# 第三方库导入
from PySide6.QtCore import QObject, QUrl, Signal, Slot
from PySide6.QtNetwork import QAbstractSocket, QNetworkRequest
from PySide6.QtWebSockets import QWebSocket

# 项目内模块导入
from src.desktop.core.api_debug.models import (
    ApiDebugActionSession,
    ApiDebugAuthConfig,
    ApiDebugAuthType,
    ApiDebugWebSocketDirection,
    ApiDebugWebSocketMessage,
    ApiDebugWebSocketState,
)

SocketFactory = Callable[[], object]


class ApiDebugWebSocketService(QObject):
    """统一封装 OneBot 直连与 DebugAdapter WebSocket。"""

    state_changed = Signal(str)
    message_logged = Signal(object)
    error_occurred = Signal(str)

    def __init__(self, parent: QObject | None = None, *, socket_factory: SocketFactory | None = None) -> None:
        super().__init__(parent)
        self.socket_factory = socket_factory or QWebSocket
        self.socket: object | None = None
        self.current_state = ApiDebugWebSocketState.DISCONNECTED
        self.current_url = ""

    def connect_onebot(self, url: str, auth_config: ApiDebugAuthConfig | None = None) -> None:
        """连接 OneBot WebSocket。"""
        self._open(url, auth_config=auth_config)

    def connect_debug_adapter(self, session: ApiDebugActionSession) -> None:
        """连接 DebugAdapter WebSocket。"""
        self._open(
            session.websocket_url,
            auth_config=None,
            extra_query={"adapterName": session.adapter_name, "token": session.token},
        )

    def disconnect(self) -> None:
        """断开连接。"""
        if self.socket is None:
            self._set_state(ApiDebugWebSocketState.DISCONNECTED)
            return

        socket = self.socket
        self.socket = None
        try:
            socket.close()
        except Exception:
            pass
        if hasattr(socket, "deleteLater"):
            socket.deleteLater()
        self.current_url = ""
        self._set_state(ApiDebugWebSocketState.DISCONNECTED)

    def send_text(self, text: str) -> None:
        """发送文本消息。"""
        if self.socket is None:
            raise RuntimeError("WebSocket 尚未连接")

        payload = str(text)
        self.socket.sendTextMessage(payload)
        self.message_logged.emit(self._build_message(ApiDebugWebSocketDirection.OUTGOING, payload))

    def send_json(self, payload: Any) -> None:
        """发送 JSON 消息。"""
        self.send_text(json.dumps(payload, ensure_ascii=False))

    def _open(
        self,
        url: str,
        *,
        auth_config: ApiDebugAuthConfig | None = None,
        extra_query: dict[str, str] | None = None,
    ) -> None:
        self.disconnect()
        self.socket = self.socket_factory()
        self.current_url = str(url)
        self._bind_socket(self.socket)

        request = self._build_request(str(url), auth_config=auth_config, extra_query=extra_query)
        self._set_state(ApiDebugWebSocketState.CONNECTING)
        self.socket.open(request)

    def _bind_socket(self, socket: object) -> None:
        socket.connected.connect(self._on_connected)
        socket.disconnected.connect(self._on_disconnected)
        socket.textMessageReceived.connect(self._on_text_message)
        socket.binaryMessageReceived.connect(self._on_binary_message)
        socket.stateChanged.connect(self._on_socket_state_changed)
        if hasattr(socket, "errorOccurred"):
            socket.errorOccurred.connect(self._on_error)

    def _build_request(
        self,
        url: str,
        *,
        auth_config: ApiDebugAuthConfig | None = None,
        extra_query: dict[str, str] | None = None,
    ) -> QNetworkRequest:
        request_url = self._compose_url(url, auth_config=auth_config, extra_query=extra_query)
        request = QNetworkRequest(QUrl(request_url))

        if auth_config is None:
            return request

        header_value = self._authorization_header(auth_config)
        if header_value:
            request.setRawHeader(b"Authorization", header_value.encode("utf-8"))

        return request

    def _compose_url(
        self,
        url: str,
        *,
        auth_config: ApiDebugAuthConfig | None,
        extra_query: dict[str, str] | None,
    ) -> str:
        parts = urlsplit(url)
        query = dict(parse_qsl(parts.query, keep_blank_values=True))

        if extra_query:
            for key, value in extra_query.items():
                if value:
                    query[str(key)] = str(value)

        if auth_config is not None and auth_config.use_query_token:
            token = auth_config.token.strip()
            if auth_config.auth_type == ApiDebugAuthType.BEARER_TOKEN and token:
                query["access_token"] = token
            elif auth_config.auth_type == ApiDebugAuthType.WEBUI_SESSION and token:
                query["webui_token"] = token

        return urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(query), parts.fragment))

    @staticmethod
    def _authorization_header(auth_config: ApiDebugAuthConfig) -> str:
        if auth_config.use_query_token:
            return ""
        if auth_config.auth_type == ApiDebugAuthType.BEARER_TOKEN and auth_config.token.strip():
            return f"Bearer {auth_config.token.strip()}"
        if auth_config.auth_type == ApiDebugAuthType.WEBUI_SESSION:
            credential = auth_config.session_credential.strip()
            if credential:
                return f"Bearer {credential}"
        return ""

    def _set_state(self, state: ApiDebugWebSocketState) -> None:
        if self.current_state == state:
            return
        self.current_state = state
        self.state_changed.emit(state.value)

    def _build_message(self, direction: ApiDebugWebSocketDirection, payload_text: str, *, note: str = ""):
        parsed_json = None
        try:
            parsed_json = json.loads(payload_text)
        except (TypeError, ValueError):
            parsed_json = None
        return ApiDebugWebSocketMessage(
            message_id=uuid.uuid4().hex,
            direction=direction,
            payload_text=payload_text,
            parsed_json=parsed_json,
            note=note,
        )

    @Slot()
    def _on_connected(self) -> None:
        self._set_state(ApiDebugWebSocketState.CONNECTED)
        self.message_logged.emit(
            self._build_message(ApiDebugWebSocketDirection.SYSTEM, "WebSocket 已连接", note=self.current_url)
        )

    @Slot()
    def _on_disconnected(self) -> None:
        self._set_state(ApiDebugWebSocketState.DISCONNECTED)
        self.message_logged.emit(self._build_message(ApiDebugWebSocketDirection.SYSTEM, "WebSocket 已断开"))

    @Slot(str)
    def _on_text_message(self, message: str) -> None:
        self.message_logged.emit(self._build_message(ApiDebugWebSocketDirection.INCOMING, message))

    @Slot(bytes)
    def _on_binary_message(self, payload: bytes) -> None:
        preview = payload.decode("utf-8", errors="replace")
        self.message_logged.emit(
            self._build_message(ApiDebugWebSocketDirection.INCOMING, preview, note=f"binary:{len(payload)} bytes")
        )

    @Slot(int)
    def _on_socket_state_changed(self, state: int) -> None:
        state_code = self._socket_state_code(state)
        mapping = {
            self._socket_state_code(QAbstractSocket.SocketState.UnconnectedState): ApiDebugWebSocketState.DISCONNECTED,
            self._socket_state_code(QAbstractSocket.SocketState.HostLookupState): ApiDebugWebSocketState.CONNECTING,
            self._socket_state_code(QAbstractSocket.SocketState.ConnectingState): ApiDebugWebSocketState.CONNECTING,
            self._socket_state_code(QAbstractSocket.SocketState.ConnectedState): ApiDebugWebSocketState.CONNECTED,
            self._socket_state_code(QAbstractSocket.SocketState.BoundState): ApiDebugWebSocketState.CONNECTING,
            self._socket_state_code(QAbstractSocket.SocketState.ClosingState): ApiDebugWebSocketState.CONNECTING,
            self._socket_state_code(QAbstractSocket.SocketState.ListeningState): ApiDebugWebSocketState.CONNECTED,
        }
        self._set_state(mapping.get(state_code, self.current_state))

    @Slot(object)
    def _on_error(self, _error: object) -> None:
        self._set_state(ApiDebugWebSocketState.ERROR)
        if self.socket is not None and hasattr(self.socket, "errorString"):
            message = self.socket.errorString()
        else:
            message = "WebSocket 连接失败"
        self.error_occurred.emit(message)
        self.message_logged.emit(self._build_message(ApiDebugWebSocketDirection.ERROR, message))

    @staticmethod
    def _socket_state_code(state: object) -> int:
        raw_state = getattr(state, "value", state)
        return int(raw_state)
