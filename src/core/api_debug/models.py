# -*- coding: utf-8 -*-
"""接口调试核心数据模型。"""

# 标准库导入
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Any


def _utc_now() -> datetime:
    return datetime.now(timezone.utc)


def _utc_now_iso() -> str:
    return _utc_now().isoformat()


class ApiDebugBodyType(str, Enum):
    """请求体类型。"""

    NONE = "none"
    JSON = "json"
    TEXT = "text"
    FORM = "form"
    BYTES = "bytes"


class ApiDebugAuthType(str, Enum):
    """认证模式。"""

    NONE = "none"
    BEARER_TOKEN = "bearer_token"
    WEBUI_SESSION = "webui_session"


class ApiDebugResponseBodyType(str, Enum):
    """响应体类型。"""

    EMPTY = "empty"
    JSON = "json"
    TEXT = "text"
    BINARY = "binary"


class ApiDebugErrorKind(str, Enum):
    """统一错误分类。"""

    REQUEST_BUILD = "request_build"
    AUTH = "auth"
    NETWORK = "network"
    TIMEOUT = "timeout"
    HTTP_STATUS = "http_status"
    HISTORY = "history"
    UNKNOWN = "unknown"


class ApiDebugMode(str, Enum):
    """工作台模式。"""

    HTTP = "http"
    ACTION = "action"
    WEBSOCKET = "websocket"


class ApiDebugTargetType(str, Enum):
    """目标端点类型。"""

    MANUAL_HTTP = "manual_http"
    ONEBOT_HTTP = "onebot_http"
    WEBUI_HTTP = "webui_http"
    ONEBOT_WEBSOCKET = "onebot_websocket"
    DEBUG_WEBSOCKET = "debug_websocket"


class ApiDebugWebSocketState(str, Enum):
    """WebSocket 连接状态。"""

    DISCONNECTED = "disconnected"
    CONNECTING = "connecting"
    CONNECTED = "connected"
    ERROR = "error"


class ApiDebugWebSocketDirection(str, Enum):
    """WebSocket 消息方向。"""

    OUTGOING = "outgoing"
    INCOMING = "incoming"
    SYSTEM = "system"
    ERROR = "error"


@dataclass(slots=True)
class ApiDebugRequestConfig:
    """接口调试请求输入配置。"""

    url: str
    method: str = "GET"
    headers: dict[str, str] = field(default_factory=dict)
    query_params: dict[str, str] = field(default_factory=dict)
    body_type: ApiDebugBodyType = ApiDebugBodyType.NONE
    body: Any = None
    timeout: float = 10.0
    follow_redirects: bool = True


@dataclass(slots=True)
class ApiDebugBuiltRequest:
    """归一化后的请求对象。"""

    method: str
    url: str
    headers: dict[str, str] = field(default_factory=dict)
    query_params: dict[str, str] = field(default_factory=dict)
    body_type: ApiDebugBodyType = ApiDebugBodyType.NONE
    body: Any = None
    timeout: float = 10.0
    follow_redirects: bool = True

    def to_httpx_kwargs(self) -> dict[str, Any]:
        """转换为 httpx.request 所需参数。"""
        kwargs: dict[str, Any] = {
            "method": self.method,
            "url": self.url,
            "headers": self.headers,
            "params": self.query_params,
        }

        if self.body_type == ApiDebugBodyType.JSON:
            kwargs["json"] = self.body
        elif self.body_type == ApiDebugBodyType.FORM:
            kwargs["data"] = self.body
        elif self.body_type in {ApiDebugBodyType.TEXT, ApiDebugBodyType.BYTES}:
            kwargs["content"] = self.body

        return kwargs


@dataclass(slots=True)
class ApiDebugAuthConfig:
    """接口调试认证配置。"""

    auth_type: ApiDebugAuthType = ApiDebugAuthType.NONE
    token: str = ""
    session_credential: str = ""
    use_query_token: bool = False

    @classmethod
    def bearer(cls, token: str, *, use_query_token: bool = False) -> "ApiDebugAuthConfig":
        """构造 Bearer Token 认证配置。"""
        return cls(auth_type=ApiDebugAuthType.BEARER_TOKEN, token=token, use_query_token=use_query_token)

    @classmethod
    def webui_session(
        cls,
        token: str = "",
        *,
        session_credential: str = "",
        use_query_token: bool = False,
    ) -> "ApiDebugAuthConfig":
        """构造 WebUI Session 认证配置。"""
        return cls(
            auth_type=ApiDebugAuthType.WEBUI_SESSION,
            token=token,
            session_credential=session_credential,
            use_query_token=use_query_token,
        )

    def describe(self) -> str:
        """生成适合 UI 展示的认证摘要。"""
        if self.auth_type == ApiDebugAuthType.NONE:
            return "无认证"
        if self.auth_type == ApiDebugAuthType.BEARER_TOKEN:
            return "Query Token" if self.use_query_token else "Bearer Token"
        if self.auth_type == ApiDebugAuthType.WEBUI_SESSION:
            return "WebUI Token" if self.use_query_token else "WebUI Session"
        return self.auth_type.value


@dataclass(slots=True)
class ApiDebugResponse:
    """响应解析结果。"""

    status_code: int
    reason_phrase: str
    headers: dict[str, str]
    body_type: ApiDebugResponseBodyType
    formatted_body: str
    json_body: Any = None
    elapsed_ms: float = 0.0
    size_bytes: int = 0


@dataclass(slots=True)
class ApiDebugError:
    """统一错误模型。"""

    kind: ApiDebugErrorKind
    message: str
    status_code: int | None = None
    details: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class ApiDebugExecutionResult:
    """单次调试请求执行结果。"""

    request: ApiDebugBuiltRequest
    response: ApiDebugResponse | None = None
    error: ApiDebugError | None = None
    started_at: datetime = field(default_factory=_utc_now)
    completed_at: datetime = field(default_factory=_utc_now)
    history_id: str | None = None

    @property
    def is_success(self) -> bool:
        """是否成功。"""
        return self.error is None


@dataclass(slots=True)
class ApiDebugHistoryRequestSnapshot:
    """持久化的请求快照。"""

    method: str
    url: str
    headers: dict[str, str]
    query_params: dict[str, str]
    body_type: str
    body: Any = None


@dataclass(slots=True)
class ApiDebugHistoryResponseSnapshot:
    """持久化的响应快照。"""

    status_code: int
    reason_phrase: str
    headers: dict[str, str]
    body_type: str
    formatted_body: str
    elapsed_ms: float
    size_bytes: int


@dataclass(slots=True)
class ApiDebugHistoryEntry:
    """持久化的历史记录条目。"""

    history_id: str
    created_at: str
    request: ApiDebugHistoryRequestSnapshot
    response: ApiDebugHistoryResponseSnapshot | None = None
    error: ApiDebugError | None = None


@dataclass(slots=True)
class ApiDebugEndpointSummary:
    """Bot 下可直接使用的端点摘要。"""

    endpoint_id: str
    name: str
    url: str
    target_type: ApiDebugTargetType
    auth_config: ApiDebugAuthConfig = field(default_factory=ApiDebugAuthConfig)
    description: str = ""
    available: bool = True

    def describe(self) -> str:
        """生成适合 UI 展示的端点摘要。"""
        target_label = {
            ApiDebugTargetType.MANUAL_HTTP: "手动",
            ApiDebugTargetType.ONEBOT_HTTP: "OneBot HTTP",
            ApiDebugTargetType.WEBUI_HTTP: "WebUI",
            ApiDebugTargetType.ONEBOT_WEBSOCKET: "OneBot WS",
            ApiDebugTargetType.DEBUG_WEBSOCKET: "Debug WS",
        }.get(self.target_type, self.target_type.value)
        return f"{target_label} · {self.url}"


@dataclass(slots=True)
class ApiDebugBotContext:
    """调试页面使用的 Bot 运行上下文。"""

    bot_id: str
    bot_name: str
    http_targets: list[ApiDebugEndpointSummary] = field(default_factory=list)
    websocket_targets: list[ApiDebugEndpointSummary] = field(default_factory=list)
    webui_base_url: str = ""
    webui_token: str = ""
    webui_credential: str = ""
    notes: list[str] = field(default_factory=list)

    def preferred_http_target(self) -> ApiDebugEndpointSummary | None:
        """优先返回 OneBot HTTP，其次 WebUI。"""
        for target in self.http_targets:
            if target.available and target.target_type == ApiDebugTargetType.ONEBOT_HTTP:
                return target
        for target in self.http_targets:
            if target.available and target.target_type == ApiDebugTargetType.WEBUI_HTTP:
                return target
        for target in self.http_targets:
            if target.available:
                return target
        return None

    def preferred_action_base_url(self) -> str:
        """返回 Action 调试默认使用的 WebUI 根地址。"""
        if self.webui_base_url:
            return self.webui_base_url.rstrip("/")
        for target in self.http_targets:
            if target.target_type == ApiDebugTargetType.WEBUI_HTTP and target.available:
                return target.url.rstrip("/")
        return ""

    def preferred_websocket_target(self) -> ApiDebugEndpointSummary | None:
        """优先返回 OneBot WS 端点。"""
        for target in self.websocket_targets:
            if target.available and target.target_type == ApiDebugTargetType.ONEBOT_WEBSOCKET:
                return target
        for target in self.websocket_targets:
            if target.available:
                return target
        return None


@dataclass(slots=True)
class ApiDebugActionSchema:
    """Action 元数据。"""

    action: str
    summary: str = ""
    description: str = ""
    payload_schema: Any = None
    return_schema: Any = None
    payload_example: Any = None
    action_tags: list[str] = field(default_factory=list)

    @property
    def title(self) -> str:
        """优先显示摘要，否则回退到 action 名称。"""
        return self.summary.strip() or self.action


@dataclass(slots=True)
class ApiDebugActionSession:
    """DebugAdapter 会话。"""

    adapter_name: str
    token: str
    base_url: str
    created_at: str = field(default_factory=_utc_now_iso)

    @property
    def websocket_url(self) -> str:
        """返回 Debug WS 地址。"""
        base = self.base_url.rstrip("/")
        if base.startswith("https://"):
            return f"wss://{base[len('https://'):]}/api/Debug/ws"
        if base.startswith("http://"):
            return f"ws://{base[len('http://'):]}/api/Debug/ws"
        return f"{base}/api/Debug/ws"


@dataclass(slots=True)
class ApiDebugHttpDraft:
    """HTTP 模式草稿。"""

    url: str = ""
    method: str = "GET"
    headers: dict[str, str] = field(default_factory=dict)
    query_params: dict[str, str] = field(default_factory=dict)
    body_type: ApiDebugBodyType = ApiDebugBodyType.NONE
    body: Any = None
    auth: ApiDebugAuthConfig = field(default_factory=ApiDebugAuthConfig)
    endpoint_id: str = ""
    target_type: ApiDebugTargetType = ApiDebugTargetType.MANUAL_HTTP


@dataclass(slots=True)
class ApiDebugActionDraft:
    """Action 模式草稿。"""

    action: str = ""
    search_query: str = ""
    params_text: str = "{}"
    endpoint_id: str = ""


@dataclass(slots=True)
class ApiDebugWebSocketDraft:
    """WebSocket 模式草稿。"""

    url: str = ""
    message_text: str = "{}"
    auth: ApiDebugAuthConfig = field(default_factory=ApiDebugAuthConfig)
    endpoint_id: str = ""
    target_type: ApiDebugTargetType = ApiDebugTargetType.ONEBOT_WEBSOCKET
    auto_scroll: bool = True


@dataclass(slots=True)
class ApiDebugPreset:
    """用户保存的预设。"""

    preset_id: str
    name: str
    mode: ApiDebugMode
    payload: dict[str, Any]
    summary: str = ""
    created_at: str = field(default_factory=_utc_now_iso)


@dataclass(slots=True)
class ApiDebugWorkspaceState:
    """工作台持久化状态。"""

    selected_bot_id: str = ""
    selected_mode: ApiDebugMode = ApiDebugMode.HTTP
    root_splitter_sizes: dict[str, list[int]] = field(default_factory=dict)
    detail_splitter_sizes: dict[str, list[int]] = field(default_factory=dict)
    http_draft: ApiDebugHttpDraft = field(default_factory=ApiDebugHttpDraft)
    action_draft: ApiDebugActionDraft = field(default_factory=ApiDebugActionDraft)
    websocket_draft: ApiDebugWebSocketDraft = field(default_factory=ApiDebugWebSocketDraft)
    presets: list[ApiDebugPreset] = field(default_factory=list)


@dataclass(slots=True)
class ApiDebugWebSocketMessage:
    """WebSocket 消息条目。"""

    message_id: str
    direction: ApiDebugWebSocketDirection
    payload_text: str
    created_at: str = field(default_factory=_utc_now_iso)
    parsed_json: Any = None
    note: str = ""


@dataclass(slots=True)
class ApiDebugSearchItem:
    """全局搜索结果项。"""

    item_id: str
    title: str
    subtitle: str
    mode: ApiDebugMode
    payload: dict[str, Any] = field(default_factory=dict)
