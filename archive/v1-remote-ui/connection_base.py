# -*- coding: utf-8 -*-
"""远程连接基类定义。

定义统一的连接接口，SSH 和 Agent 模式都实现此接口。
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum, auto
from typing import Any, Callable

from PySide6.QtCore import QObject, Signal


class ConnectionState(Enum):
    """连接状态。"""
    DISCONNECTED = auto()
    CONNECTING = auto()
    AUTHENTICATING = auto()
    CONNECTED = auto()
    ERROR = auto()
    RECONNECTING = auto()


@dataclass
class ConnectionInfo:
    """连接信息。"""
    host: str
    port: int
    mode: str  # "ssh" | "agent"
    username: str | None = None

    @property
    def display_name(self) -> str:
        if self.username:
            return f"{self.username}@{self.host}:{self.port}"
        return f"{self.host}:{self.port}"


@dataclass
class ServerStatus:
    """服务器状态。"""
    connected: bool = False
    os_name: str = "--"
    os_version: str = "--"
    architecture: str = "--"
    kernel: str = "--"
    hostname: str = "--"
    napcat_running: bool = False
    napcat_version: str | None = None
    napcat_pid: int | None = None
    qq_version: str | None = None
    qq_number: str | None = None
    uptime: int = 0  # 秒
    daemon_version: str | None = None


class BaseConnectionHandler(QObject):
    """连接处理器基类。

    Signals:
        state_changed: 状态变化 (ConnectionState)
        status_updated: 状态信息更新 (ServerStatus)
        log_received: 收到日志消息 (str)
        error_occurred: 发生错误 (str)
        progress_updated: 进度更新 (message: str, percent: int)
    """

    state_changed = Signal(object)  # ConnectionState
    status_updated = Signal(object)  # ServerStatus
    log_received = Signal(str)
    error_occurred = Signal(str)
    progress_updated = Signal(str, int)

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._state = ConnectionState.DISCONNECTED
        self._info: ConnectionInfo | None = None
        self._status = ServerStatus()

    @property
    def state(self) -> ConnectionState:
        return self._state

    @property
    def is_connected(self) -> bool:
        return self._state == ConnectionState.CONNECTED

    @property
    def connection_info(self) -> ConnectionInfo | None:
        return self._info

    @property
    def server_status(self) -> ServerStatus:
        return self._status

    def _set_state(self, state: ConnectionState) -> None:
        """设置状态并发射信号。"""
        if self._state != state:
            self._state = state
            self.state_changed.emit(state)

    def _update_status(self, **kwargs) -> None:
        """更新状态并发射信号。"""
        for key, value in kwargs.items():
            if hasattr(self._status, key):
                setattr(self._status, key, value)
        self.status_updated.emit(self._status)

    @abstractmethod
    def connect(self, info: ConnectionInfo, **kwargs) -> None:
        """建立连接。

        Args:
            info: 连接信息
            **kwargs: 额外参数（如密码、token等）
        """
        pass

    @abstractmethod
    def disconnect(self) -> None:
        """断开连接。"""
        pass

    @abstractmethod
    def test_connection(self) -> tuple[bool, str]:
        """测试连接。

        Returns:
            (是否成功, 消息)
        """
        pass

    @abstractmethod
    def start_napcat(self) -> tuple[bool, str]:
        """启动 NapCat。

        Returns:
            (是否成功, 消息)
        """
        pass

    @abstractmethod
    def stop_napcat(self) -> tuple[bool, str]:
        """停止 NapCat。

        Returns:
            (是否成功, 消息)
        """
        pass

    @abstractmethod
    def restart_napcat(self) -> tuple[bool, str]:
        """重启 NapCat。

        Returns:
            (是否成功, 消息)
        """
        pass

    @abstractmethod
    def get_logs(self, lines: int = 100) -> list[str]:
        """获取日志。

        Args:
            lines: 行数

        Returns:
            日志行列表
        """
        pass

    @abstractmethod
    def deploy(self, progress_callback: Callable[[str, int], None] | None = None) -> tuple[bool, str]:
        """部署 NapCat。

        Args:
            progress_callback: 进度回调 (message, percent)

        Returns:
            (是否成功, 消息)
        """
        pass

    @abstractmethod
    def save_config(self) -> None:
        """保存配置。"""
        pass
