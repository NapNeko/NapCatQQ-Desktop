# -*- coding: utf-8 -*-
"""Agent WebSocket 客户端（JSON-RPC 2.0）。

提供与 Go Daemon 的 WebSocket 通信能力。
"""

from __future__ import annotations

import asyncio
import json
import logging
import uuid
from dataclasses import dataclass
from typing import Any, Callable

from PySide6.QtCore import QObject, Signal
from PySide6.QtWebSockets import QWebSocket
from PySide6.QtNetwork import QAbstractSocket

from .jsonrpc_protocol import (
    NapCatMethod,
    NapCatNotification,
    JsonRpcRequest,
    JsonRpcResponse,
    JsonRpcNotification,
    NapCatStatus,
    LogEntry,
    ErrorCode,
    JsonRpcError,
)

logger = logging.getLogger(__name__)


@dataclass(slots=True)
class AgentConnectionConfig:
    """Agent 连接配置。"""

    host: str = "localhost"
    port: int = 8080
    token: str = ""
    use_ssl: bool = False
    auto_reconnect: bool = True
    reconnect_interval: float = 5.0
    request_timeout: float = 30.0

    @property
    def ws_url(self) -> str:
        """WebSocket URL。"""
        protocol = "wss" if self.use_ssl else "ws"
        return f"{protocol}://{self.host}:{self.port}/ws"

    @property
    def http_url(self) -> str:
        """HTTP URL。"""
        protocol = "https" if self.use_ssl else "http"
        return f"{protocol}://{self.host}:{self.port}"


class AgentClient(QObject):
    """Agent WebSocket 客户端（JSON-RPC 2.0）。

    Signals:
        connected: 连接建立时发射
        disconnected: 连接断开时发射
        authenticated: 认证成功时发射
        error: 发生错误时发射 (error_message)
        status_changed: NapCat 状态变化时发射 (NapCatStatus)
        log_received: 收到日志时发射 (LogEntry)
        notification: 收到任意通知时发射 (JsonRpcNotification)
    """

    connected = Signal()
    disconnected = Signal()
    authenticated = Signal()
    error = Signal(str)
    status_changed = Signal(object)  # NapCatStatus
    log_received = Signal(object)  # LogEntry
    notification = Signal(object)  # JsonRpcNotification

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)

        self._config: AgentConnectionConfig | None = None
        self._ws: QWebSocket | None = None
        self._is_connected: bool = False
        self._is_authenticated: bool = False
        self._pending_requests: dict[str | int, asyncio.Future] = {}
        self._event_handlers: dict[str, list[Callable[[Any], None]]] = {}

        # Reconnection state
        self._should_reconnect: bool = False

    @property
    def is_connected(self) -> bool:
        """是否已连接。"""
        return self._is_connected

    @property
    def is_authenticated(self) -> bool:
        """是否已认证。"""
        return self._is_authenticated

    @property
    def config(self) -> AgentConnectionConfig | None:
        """当前配置。"""
        return self._config

    def connect_to_agent(self, config: AgentConnectionConfig) -> None:
        """连接到 Agent。

        Args:
            config: 连接配置
        """
        self._config = config
        self._should_reconnect = config.auto_reconnect

        self._ws = QWebSocket()
        self._setup_websocket_handlers()

        logger.info(f"Connecting to Agent at {config.ws_url}")
        self._ws.open(config.ws_url)

    def disconnect_from_agent(self) -> None:
        """断开连接。"""
        self._should_reconnect = False

        if self._ws:
            self._ws.close()
            self._ws = None

        self._cleanup_state()

    def _cleanup_state(self) -> None:
        """清理状态。"""
        self._is_connected = False
        self._is_authenticated = False

        # Cancel all pending requests
        for future in self._pending_requests.values():
            if not future.done():
                future.set_exception(ConnectionError("Connection closed"))
        self._pending_requests.clear()

    def _setup_websocket_handlers(self) -> None:
        """设置 WebSocket 事件处理器。"""
        if not self._ws:
            return

        self._ws.connected.connect(self._on_connected)
        self._ws.disconnected.connect(self._on_disconnected)
        self._ws.textMessageReceived.connect(self._on_text_message)
        self._ws.error.connect(self._on_error)

    def _on_connected(self) -> None:
        """连接建立回调。"""
        logger.info("Agent connection established")
        self._is_connected = True
        self.connected.emit()

        # Send authentication
        self._authenticate()

    def _on_disconnected(self) -> None:
        """连接断开回调。"""
        logger.info("Agent connection closed")
        was_connected = self._is_connected
        self._cleanup_state()
        self.disconnected.emit()

        # Schedule reconnect if needed
        if was_connected and self._should_reconnect and self._config:
            self._schedule_reconnect()

    def _on_error(self, error_code: QAbstractSocket.SocketError) -> None:
        """错误回调。"""
        error_msg = f"WebSocket error: {error_code}"
        if self._ws:
            error_msg = f"{error_msg} - {self._ws.errorString()}"
        logger.error(error_msg)
        self.error.emit(error_msg)

    def _on_text_message(self, message: str) -> None:
        """收到文本消息回调。"""
        try:
            data = json.loads(message)
        except json.JSONDecodeError as e:
            logger.warning(f"Failed to parse message: {e}")
            return

        # Validate JSON-RPC version
        if data.get("jsonrpc") != "2.0":
            logger.warning(f"Invalid JSON-RPC version: {data.get('jsonrpc')}")
            return

        # Check if it's a notification (has method, no id) or response (has id)
        if "method" in data and "id" not in data:
            self._handle_notification(data)
        elif "id" in data:
            self._handle_response(data)
        else:
            logger.warning(f"Unknown message format: {data}")

    def _handle_notification(self, data: dict[str, Any]) -> None:
        """处理通知消息（服务端主动推送）。"""
        try:
            notification = JsonRpcNotification.from_dict(data)
        except Exception as e:
            logger.warning(f"Failed to parse notification: {e}")
            return

        self.notification.emit(notification)

        # Handle specific notifications
        if notification.method == NapCatNotification.STATUS_UPDATE:
            if isinstance(notification.params, dict):
                status = NapCatStatus.from_dict(notification.params)
                self.status_changed.emit(status)
        elif notification.method == NapCatNotification.LOG_ENTRY:
            if isinstance(notification.params, dict):
                log_entry = LogEntry.from_dict(notification.params)
                self.log_received.emit(log_entry)

        # Call registered handlers
        handlers = self._event_handlers.get(notification.method, [])
        for handler in handlers:
            try:
                handler(notification.params)
            except Exception as e:
                logger.exception(f"Event handler error: {e}")

    def _handle_response(self, data: dict[str, Any]) -> None:
        """处理响应消息。"""
        try:
            response = JsonRpcResponse.from_dict(data)
        except Exception as e:
            logger.warning(f"Failed to parse response: {e}")
            return

        # Check if it's a pending request
        future = self._pending_requests.pop(response.id, None)
        if future and not future.done():
            future.set_result(response)

        # Handle authentication response (id="auth")
        if response.id == "auth":
            if response.is_success:
                self._is_authenticated = True
                self.authenticated.emit()
            else:
                error_msg = f"Authentication failed: {response.error.message if response.error else 'Unknown'}"
                logger.error(error_msg)
                self.error.emit(error_msg)

    def _authenticate(self) -> None:
        """发送认证请求。"""
        if not self._config:
            return

        request = JsonRpcRequest(
            id="auth",
            method=NapCatMethod.AUTH_AUTHENTICATE,
            params={"token": self._config.token},
        )
        self._send_request(request)

    def _send_request(self, request: JsonRpcRequest) -> None:
        """发送请求。"""
        if not self._ws or not self._is_connected:
            raise ConnectionError("Not connected to agent")

        data = json.dumps(request.to_dict())
        self._ws.sendTextMessage(data)

    def _schedule_reconnect(self) -> None:
        """计划重连。"""
        if not self._config or not self._should_reconnect:
            return

        logger.info(f"Scheduling reconnect in {self._config.reconnect_interval}s")

        # Use QTimer for Qt-based reconnection
        from PySide6.QtCore import QTimer
        QTimer.singleShot(
            int(self._config.reconnect_interval * 1000),
            self._do_reconnect
        )

    def _do_reconnect(self) -> None:
        """执行重连。"""
        if not self._should_reconnect or not self._config:
            return

        if not self._is_connected:
            logger.info("Attempting to reconnect...")
            self.connect_to_agent(self._config)

    async def request(
        self,
        method: str,
        params: dict[str, Any] | None = None,
        timeout: float | None = None,
    ) -> JsonRpcResponse:
        """发送 JSON-RPC 请求并等待响应。

        Args:
            method: 请求方法
            params: 请求参数
            timeout: 超时时间（秒），None 使用配置默认值

        Returns:
            JsonRpcResponse: 响应对象

        Raises:
            ConnectionError: 未连接时
            TimeoutError: 请求超时
        """
        if not self._is_connected:
            raise ConnectionError("Not connected to agent")

        if not self._is_authenticated and method != NapCatMethod.AUTH_AUTHENTICATE:
            raise ConnectionError("Not authenticated")

        request_id = str(uuid.uuid4())
        request = JsonRpcRequest(
            id=request_id,
            method=method,
            params=params or {},
        )

        # Create future for response
        loop = asyncio.get_event_loop()
        future: asyncio.Future[JsonRpcResponse] = loop.create_future()
        self._pending_requests[request_id] = future

        try:
            self._send_request(request)

            # Wait for response
            timeout_val = timeout or (self._config.request_timeout if self._config else 30.0)
            response = await asyncio.wait_for(future, timeout=timeout_val)
            return response

        except asyncio.TimeoutError:
            self._pending_requests.pop(request_id, None)
            raise TimeoutError(f"Request {method} timed out after {timeout_val}s")

        except Exception:
            self._pending_requests.pop(request_id, None)
            raise

    # Convenience methods

    async def start_napcat(self, work_dir: str | None = None) -> dict[str, Any]:
        """启动 NapCat。

        Args:
            work_dir: 工作目录

        Returns:
            启动结果
        """
        params: dict[str, Any] = {}
        if work_dir:
            params["work_dir"] = work_dir

        response = await self.request(NapCatMethod.NAPCAT_START, params)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return response.result or {}

    async def stop_napcat(self) -> dict[str, Any]:
        """停止 NapCat。

        Returns:
            停止结果
        """
        response = await self.request(NapCatMethod.NAPCAT_STOP)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return response.result or {}

    async def restart_napcat(self, work_dir: str | None = None) -> dict[str, Any]:
        """重启 NapCat。

        Args:
            work_dir: 工作目录

        Returns:
            重启结果
        """
        params: dict[str, Any] = {}
        if work_dir:
            params["work_dir"] = work_dir

        response = await self.request(NapCatMethod.NAPCAT_RESTART, params)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return response.result or {}

    async def get_status(self) -> NapCatStatus:
        """获取 NapCat 状态。

        Returns:
            当前状态
        """
        response = await self.request(NapCatMethod.NAPCAT_STATUS)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return NapCatStatus.from_dict(response.result or {"running": False})

    async def get_logs(self) -> list[str]:
        """获取 NapCat 日志。

        Returns:
            日志行列表
        """
        response = await self.request(NapCatMethod.NAPCAT_LOGS)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        result = response.result or {}
        return result.get("logs", [])

    async def subscribe_logs(self, level: str | None = None) -> None:
        """订阅日志流。

        Args:
            level: 日志级别过滤
        """
        params: dict[str, Any] = {}
        if level:
            params["level"] = level

        response = await self.request(NapCatMethod.LOG_SUBSCRIBE, params)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")

    async def unsubscribe_logs(self) -> None:
        """取消订阅日志流。"""
        response = await self.request(NapCatMethod.LOG_UNSUBSCRIBE)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")

    async def get_config(self, key: str | None = None) -> dict[str, Any]:
        """获取配置。

        Args:
            key: 配置键，None 返回全部

        Returns:
            配置值
        """
        params: dict[str, Any] = {}
        if key:
            params["key"] = key

        response = await self.request(NapCatMethod.CONFIG_GET, params)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return response.result or {}

    async def set_config(self, key: str, value: Any) -> None:
        """设置配置。

        Args:
            key: 配置键
            value: 配置值
        """
        response = await self.request(
            NapCatMethod.CONFIG_SET,
            {"key": key, "value": value}
        )
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")

    async def get_system_info(self) -> dict[str, Any]:
        """获取系统信息。

        Returns:
            系统信息
        """
        response = await self.request(NapCatMethod.SYSTEM_INFO)
        if not response.is_success:
            raise RuntimeError(response.error.message if response.error else "Unknown error")
        return response.result or {}

    def on_notification(self, method: str, handler: Callable[[Any], None]) -> None:
        """注册通知处理器。

        Args:
            method: 通知方法名
            handler: 处理函数
        """
        if method not in self._event_handlers:
            self._event_handlers[method] = []
        self._event_handlers[method].append(handler)

    def off_notification(self, method: str, handler: Callable[[Any], None] | None = None) -> None:
        """移除通知处理器。

        Args:
            method: 通知方法名
            handler: 处理函数，None 移除所有
        """
        if method not in self._event_handlers:
            return

        if handler is None:
            del self._event_handlers[method]
        else:
            self._event_handlers[method] = [
                h for h in self._event_handlers[method] if h != handler
            ]
