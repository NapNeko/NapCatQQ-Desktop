# -*- coding: utf-8 -*-
"""JSON-RPC 2.0 协议实现。

与 Go Daemon 的 pkg/jsonrpc 包保持同步。
https://www.jsonrpc.org/specification
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntEnum
from typing import Any


class ErrorCode(IntEnum):
    """JSON-RPC 2.0 标准错误码。"""

    PARSE_ERROR = -32700
    INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    INTERNAL_ERROR = -32603
    SERVER_ERROR_START = -32099
    SERVER_ERROR_END = -32000
    SERVER_NOT_INITIALIZED = -32002
    UNKNOWN_ERROR_CODE = -32001


# 标准错误消息
_ERROR_MESSAGES: dict[int, str] = {
    ErrorCode.PARSE_ERROR: "Parse error",
    ErrorCode.INVALID_REQUEST: "Invalid Request",
    ErrorCode.METHOD_NOT_FOUND: "Method not found",
    ErrorCode.INVALID_PARAMS: "Invalid params",
    ErrorCode.INTERNAL_ERROR: "Internal error",
    ErrorCode.SERVER_NOT_INITIALIZED: "Server not initialized",
}


@dataclass(slots=True)
class JsonRpcError:
    """JSON-RPC 2.0 错误对象。"""

    code: int
    message: str
    data: Any = None

    def __str__(self) -> str:
        return f"JSON-RPC error {self.code}: {self.message}"

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "code": self.code,
            "message": self.message,
        }
        if self.data is not None:
            result["data"] = self.data
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> JsonRpcError:
        return cls(
            code=data["code"],
            message=data["message"],
            data=data.get("data"),
        )

    @classmethod
    def from_code(cls, code: int, details: str | None = None) -> JsonRpcError:
        """从标准错误码创建错误对象。"""
        message = _ERROR_MESSAGES.get(code, "Unknown error")
        return cls(code=code, message=message, data=details)


@dataclass(slots=True)
class JsonRpcRequest:
    """JSON-RPC 2.0 请求对象。"""

    method: str
    params: dict[str, Any] = field(default_factory=dict)
    id: str | int | None = None
    jsonrpc: str = "2.0"

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "jsonrpc": self.jsonrpc,
            "method": self.method,
        }
        if self.params:
            result["params"] = self.params
        if self.id is not None:
            result["id"] = self.id
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> JsonRpcRequest:
        return cls(
            jsonrpc=data.get("jsonrpc", "2.0"),
            method=data["method"],
            params=data.get("params", {}),
            id=data.get("id"),
        )

    @property
    def is_notification(self) -> bool:
        """是否为通知（无 id 的请求不需要响应）。"""
        return self.id is None


@dataclass(slots=True)
class JsonRpcResponse:
    """JSON-RPC 2.0 响应对象。"""

    id: str | int | None
    result: Any = None
    error: JsonRpcError | None = None
    jsonrpc: str = "2.0"

    @property
    def is_success(self) -> bool:
        """是否成功响应。"""
        return self.error is None

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "jsonrpc": self.jsonrpc,
            "id": self.id,
        }
        if self.error is not None:
            result["error"] = self.error.to_dict()
        else:
            result["result"] = self.result
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> JsonRpcResponse:
        error = None
        if "error" in data:
            error = JsonRpcError.from_dict(data["error"])
        return cls(
            jsonrpc=data.get("jsonrpc", "2.0"),
            id=data.get("id"),
            result=data.get("result"),
            error=error,
        )


@dataclass(slots=True)
class JsonRpcNotification:
    """JSON-RPC 2.0 通知对象（服务端主动推送）。"""

    method: str
    params: Any = None
    jsonrpc: str = "2.0"

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "jsonrpc": self.jsonrpc,
            "method": self.method,
        }
        if self.params is not None:
            result["params"] = self.params
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> JsonRpcNotification:
        return cls(
            jsonrpc=data.get("jsonrpc", "2.0"),
            method=data["method"],
            params=data.get("params"),
        )


# NapCat 专用方法名
class NapCatMethod:
    """NapCat JSON-RPC 方法名。"""

    # Authentication
    AUTH_AUTHENTICATE = "auth.authenticate"

    # NapCat control
    NAPCAT_START = "napcat.start"
    NAPCAT_STOP = "napcat.stop"
    NAPCAT_RESTART = "napcat.restart"
    NAPCAT_STATUS = "napcat.status"
    NAPCAT_LOGS = "napcat.logs"

    # Configuration
    CONFIG_GET = "config.get"
    CONFIG_SET = "config.set"

    # Log streaming
    LOG_SUBSCRIBE = "log.subscribe"
    LOG_UNSUBSCRIBE = "log.unsubscribe"

    # File operations
    FILE_UPLOAD = "file.upload"
    FILE_DOWNLOAD = "file.download"

    # System
    SYSTEM_INFO = "system.info"


# NapCat 专用通知名
class NapCatNotification:
    """NapCat JSON-RPC 通知名。"""

    STATUS_UPDATE = "status.update"
    LOG_ENTRY = "log.entry"
    PROCESS_EXIT = "process.exit"
    PROCESS_START = "process.start"
    PROCESS_ERROR = "process.error"


# 数据模型
@dataclass(slots=True)
class NapCatStatus:
    """NapCat 运行状态。"""

    running: bool
    pid: int | None = None
    qq: str | None = None
    version: str | None = None
    log_file: str | None = None
    uptime: int = 0

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> NapCatStatus:
        # 处理嵌套结构
        if "status" in data:
            data = data["status"]
        return cls(
            running=data.get("running", False),
            pid=data.get("pid"),
            qq=data.get("qq"),
            version=data.get("version"),
            log_file=data.get("log_file"),
            uptime=data.get("uptime", 0),
        )

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {"running": self.running}
        if self.pid is not None:
            result["pid"] = self.pid
        if self.qq is not None:
            result["qq"] = self.qq
        if self.version is not None:
            result["version"] = self.version
        if self.log_file is not None:
            result["log_file"] = self.log_file
        if self.uptime:
            result["uptime"] = self.uptime
        return result


@dataclass(slots=True)
class LogEntry:
    """日志条目。"""

    timestamp: int
    level: str
    message: str
    source: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LogEntry:
        return cls(
            timestamp=data.get("timestamp", 0),
            level=data.get("level", "INFO"),
            message=data.get("message", ""),
            source=data.get("source"),
        )


@dataclass(slots=True)
class SystemInfo:
    """系统信息。"""

    version: str
    go_version: str
    os: str
    arch: str
    pid: int

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SystemInfo:
        if "info" in data:
            data = data["info"]
        return cls(
            version=data.get("version", ""),
            go_version=data.get("go_version", ""),
            os=data.get("os", ""),
            arch=data.get("arch", ""),
            pid=data.get("pid", 0),
        )
