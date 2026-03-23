# -*- coding: utf-8 -*-

# 标准库导入
from datetime import timedelta
from pathlib import Path

# 第三方库导入
import httpx

# 项目内模块导入
from src.core.api_debug import (
    ApiDebugAuthConfig,
    ApiDebugAuthInjector,
    ApiDebugBodyType,
    ApiDebugErrorKind,
    ApiDebugHistoryManager,
    ApiDebugRequestBuilder,
    ApiDebugRequestConfig,
    ApiDebugResponseBodyType,
    ApiDebugResponseParser,
    ApiDebugService,
    create_bearer_auth_from_network_config,
)


class FakeClient:
    """最小可用的 httpx.Client 替身。"""

    def __init__(self, *, response: httpx.Response | None = None, error: Exception | None = None, **kwargs) -> None:
        self.response = response
        self.error = error
        self.kwargs = kwargs
        self.requests: list[dict] = []

    def __enter__(self) -> "FakeClient":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        return False

    def request(self, **kwargs) -> httpx.Response:
        self.requests.append(kwargs)
        if self.error is not None:
            raise self.error
        assert self.response is not None
        return self.response


def test_request_builder_supports_query_headers_and_json_body() -> None:
    """请求构造器应规范化 Method、Header、Query 和 JSON Body。"""
    builder = ApiDebugRequestBuilder()

    request = builder.build(
        ApiDebugRequestConfig(
            url="http://127.0.0.1:3000/api/test",
            method=" post ",
            headers={"X-Trace": 123},
            query_params={"page": 2},
            body_type=ApiDebugBodyType.JSON,
            body='{"name":"napcat"}',
        )
    )

    assert request.method == "POST"
    assert request.headers["X-Trace"] == "123"
    assert request.headers["Content-Type"] == "application/json"
    assert request.query_params == {"page": "2"}
    assert request.body == {"name": "napcat"}


def test_auth_injector_supports_bearer_and_webui_session(monkeypatch) -> None:
    """认证注入器应兼容 Bearer Token 和 WebUI Session Credential。"""
    injector = ApiDebugAuthInjector()
    request = ApiDebugRequestBuilder().build(
        ApiDebugRequestConfig(url="http://127.0.0.1:3000/api/Debug/call", body_type=ApiDebugBodyType.JSON, body={})
    )

    bearer_request = injector.inject(request, ApiDebugAuthConfig.bearer("onebot-token"))
    assert bearer_request.headers["Authorization"] == "Bearer onebot-token"

    login_calls: list[str] = []

    def fake_post(url: str, **kwargs) -> httpx.Response:
        login_calls.append(url)
        return httpx.Response(
            200,
            json={"data": {"Credential": "session-credential"}},
            request=httpx.Request("POST", url),
        )

    monkeypatch.setattr("src.core.api_debug.auth.httpx.post", fake_post)
    session_request = injector.inject(request, ApiDebugAuthConfig.webui_session("webui-token"))

    assert session_request.headers["Authorization"] == "Bearer session-credential"
    assert login_calls == ["http://127.0.0.1:3000/api/auth/login"]


def test_create_bearer_auth_from_network_config_uses_existing_token() -> None:
    """应能直接复用现有网络配置对象里的 token。"""

    class FakeNetworkConfig:
        token = "existing-token"

    auth_config = create_bearer_auth_from_network_config(FakeNetworkConfig())

    assert auth_config == ApiDebugAuthConfig.bearer("existing-token")


def test_response_parser_formats_json_and_binary_body() -> None:
    """响应解析器应区分 JSON 与二进制响应体。"""
    parser = ApiDebugResponseParser()
    json_response = httpx.Response(
        200,
        json={"code": 0, "data": {"ok": True}},
        headers={"Content-Type": "application/json"},
        request=httpx.Request("GET", "http://127.0.0.1:3000/api/test"),
    )
    json_response.elapsed = timedelta(milliseconds=25)
    parsed_json = parser.parse(json_response)

    assert parsed_json.body_type == ApiDebugResponseBodyType.JSON
    assert '"ok": true' in parsed_json.formatted_body
    assert parsed_json.elapsed_ms == 25.0

    binary_response = httpx.Response(
        200,
        content=b"\x89PNG\x00\x01",
        headers={"Content-Type": "application/octet-stream"},
        request=httpx.Request("GET", "http://127.0.0.1:3000/file"),
    )
    parsed_binary = parser.parse(binary_response)

    assert parsed_binary.body_type == ApiDebugResponseBodyType.BINARY
    assert "<binary body>" in parsed_binary.formatted_body


def test_api_debug_service_success_persists_redacted_history(tmp_path: Path) -> None:
    """成功请求应写入历史记录，并对敏感 Header 做脱敏。"""
    response = httpx.Response(
        200,
        json={"code": 0, "message": "ok"},
        headers={"Content-Type": "application/json"},
        request=httpx.Request("POST", "http://127.0.0.1:3000/api/Debug/call"),
    )
    fake_client = FakeClient(response=response)
    history = ApiDebugHistoryManager(storage_path=tmp_path / "api_debug_history.json")
    service = ApiDebugService(
        history_manager=history,
        client_factory=lambda **kwargs: fake_client,
    )

    result = service.execute(
        ApiDebugRequestConfig(
            url="http://127.0.0.1:3000/api/Debug/call",
            method="POST",
            body_type=ApiDebugBodyType.JSON,
            body={"action": "get_status"},
        ),
        ApiDebugAuthConfig.bearer("secret-token"),
    )

    assert result.is_success is True
    assert result.history_id
    stored_entry = history.get_entry(result.history_id)
    assert stored_entry is not None
    assert stored_entry.request.headers["Authorization"] == "<redacted>"
    assert stored_entry.response is not None
    assert stored_entry.response.status_code == 200
    assert fake_client.requests[0]["headers"]["Authorization"] == "Bearer secret-token"


def test_api_debug_service_wraps_http_error_response(tmp_path: Path) -> None:
    """4xx/5xx 响应应保留响应内容并转换为 HTTP_STATUS 错误。"""
    response = httpx.Response(
        401,
        json={"code": 401, "message": "Unauthorized"},
        headers={"Content-Type": "application/json"},
        request=httpx.Request("GET", "http://127.0.0.1:3000/api/protected"),
    )
    service = ApiDebugService(
        history_manager=ApiDebugHistoryManager(storage_path=tmp_path / "history.json"),
        client_factory=lambda **kwargs: FakeClient(response=response),
    )

    result = service.execute(ApiDebugRequestConfig(url="http://127.0.0.1:3000/api/protected"))

    assert result.error is not None
    assert result.error.kind == ApiDebugErrorKind.HTTP_STATUS
    assert result.response is not None
    assert result.response.status_code == 401


def test_api_debug_service_wraps_timeout_and_persists_history(tmp_path: Path) -> None:
    """超时应统一封装，并记录失败历史。"""
    request = httpx.Request("GET", "http://127.0.0.1:3000/api/slow")
    timeout_error = httpx.ConnectTimeout("timeout", request=request)
    history = ApiDebugHistoryManager(storage_path=tmp_path / "timeout_history.json")
    service = ApiDebugService(
        history_manager=history,
        client_factory=lambda **kwargs: FakeClient(error=timeout_error),
    )

    result = service.execute(ApiDebugRequestConfig(url="http://127.0.0.1:3000/api/slow"))

    assert result.error is not None
    assert result.error.kind == ApiDebugErrorKind.TIMEOUT
    stored_entries = history.list_entries()
    assert len(stored_entries) == 1
    assert stored_entries[0].error is not None
    assert stored_entries[0].error.kind == ApiDebugErrorKind.TIMEOUT
