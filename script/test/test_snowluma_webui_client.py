# -*- coding: utf-8 -*-
"""[`SnowLumaWebUIClient.update_onebot_config`](src/core/runtime/snowluma_webui_client.py) 单元测试.

2026-05-11 问题 2 修复新增 ``update_onebot_config(uin, config)`` 方法, 走 SnowLuma
``POST /api/config/:uin`` 热推送 OneBot 配置. 上游实现见
``@example/SnowLuma-main/packages/core/src/webui/server.ts:411-427``.

测试覆盖:

- 正常推送 + ``reloaded=True`` (uin 在线);
- 推送成功但 ``reloaded=False`` (uin 不在线, 已落盘);
- ``success=false`` server 拒绝 → 抛 :class:`SnowLumaWebUIError`;
- HTTP 非 200 → 抛错;
- URL 拼接含 uin (字符串或整数都支持).

测试不发任何真实 HTTP, 全靠 :mod:`httpx` API monkeypatch.
"""
from __future__ import annotations

# 标准库导入
import json
from typing import Any

# 第三方库导入
import httpx
import pytest

# 项目内模块导入
from src.core.runtime.snowluma_webui_client import (
    SnowLumaWebUIClient,
    SnowLumaWebUIError,
)


class FakeResponse:
    """``httpx.Response`` 最小替身."""

    def __init__(self, status_code: int, payload: Any) -> None:
        self.status_code = status_code
        self._payload = payload

    @property
    def text(self) -> str:
        return json.dumps(self._payload, ensure_ascii=False)

    def json(self) -> Any:
        return self._payload


def _make_client_with_token() -> SnowLumaWebUIClient:
    """构造一个已 login 的 client (跳过 _authed_request 内部 lazy login)."""
    client = SnowLumaWebUIClient(host="127.0.0.1", port=8060, password="pwd")
    client._token = "fake-token"
    return client


# ==================== update_onebot_config 成功 ====================
def test_update_onebot_config_returns_true_when_reloaded(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """server 返回 ``{success:true, reloaded:true}`` 时, 方法返回 ``True``."""
    client = _make_client_with_token()
    captured: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        captured["method"] = method
        captured["url"] = url
        captured["json"] = kwargs.get("json")
        return FakeResponse(
            200,
            {
                "success": True,
                "reloaded": True,
                "message": "配置保存成功，已热重载当前会话。",
            },
        )

    monkeypatch.setattr(httpx, "request", fake_request)

    config_payload = {"networks": {"httpServers": []}, "musicSignUrl": ""}
    result = client.update_onebot_config(uin=114514, config=config_payload)

    assert result is True
    assert captured["method"] == "POST"
    assert captured["url"] == "http://127.0.0.1:8060/api/config/114514"
    assert captured["json"] == config_payload


def test_update_onebot_config_returns_false_when_not_reloaded(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """server 返回 ``{success:true, reloaded:false}`` (uin 不在线) 时返回 ``False``."""
    client = _make_client_with_token()

    monkeypatch.setattr(
        httpx,
        "request",
        lambda *args, **kwargs: FakeResponse(
            200,
            {
                "success": True,
                "reloaded": False,
                "message": "配置保存成功，当前会话未在线，将在下次连接时生效。",
            },
        ),
    )

    assert client.update_onebot_config(uin="223344", config={"networks": {}}) is False


def test_update_onebot_config_accepts_uin_as_string(monkeypatch: pytest.MonkeyPatch) -> None:
    """uin 传字符串时 URL 拼接正确."""
    client = _make_client_with_token()
    captured: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        captured["url"] = url
        return FakeResponse(200, {"success": True, "reloaded": True})

    monkeypatch.setattr(httpx, "request", fake_request)

    client.update_onebot_config(uin="9876543", config={"networks": {}})
    assert captured["url"].endswith("/api/config/9876543")


# ==================== update_onebot_config 失败 ====================
def test_update_onebot_config_raises_when_success_false(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """server 返回 ``{success:false}`` 时, 应抛 :class:`SnowLumaWebUIError`."""
    client = _make_client_with_token()

    monkeypatch.setattr(
        httpx,
        "request",
        lambda *args, **kwargs: FakeResponse(
            200,
            {"success": False, "message": "uin not exists"},
        ),
    )

    with pytest.raises(SnowLumaWebUIError) as exc_info:
        client.update_onebot_config(uin=114514, config={"networks": {}})
    assert "uin not exists" in exc_info.value.message


def test_update_onebot_config_raises_when_http_non_200(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """HTTP 500 时应抛 :class:`SnowLumaWebUIError` 含 status_code."""
    client = _make_client_with_token()

    monkeypatch.setattr(
        httpx,
        "request",
        lambda *args, **kwargs: FakeResponse(500, {"error": "internal"}),
    )

    with pytest.raises(SnowLumaWebUIError) as exc_info:
        client.update_onebot_config(uin=114514, config={"networks": {}})
    assert exc_info.value.status_code == 500


def test_update_onebot_config_raises_when_response_not_dict(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """server 返回非 dict (例如纯字符串 / 数组) 时, 应抛错."""
    client = _make_client_with_token()

    monkeypatch.setattr(
        httpx,
        "request",
        lambda *args, **kwargs: FakeResponse(200, ["not", "a", "dict"]),
    )

    with pytest.raises(SnowLumaWebUIError, match="响应结构异常"):
        client.update_onebot_config(uin=114514, config={"networks": {}})
