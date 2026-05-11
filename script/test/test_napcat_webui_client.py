# -*- coding: utf-8 -*-
"""[`NapCatWebUIClient`](src/core/runtime/napcat_webui_client.py) 单元测试.

覆盖 2026-05-11 问题 2 修复新增的 NapCat WebUI HTTP 客户端:

- ``_generate_password_hash`` 与 NapCat ``crypto.createHash('sha256').update(token + '.napcat')`` 一致;
- ``read_napcat_webui_config`` 读取 webui.json (存在 / 缺失 / 损坏);
- ``NapCatWebUIClient.from_napcat_path`` 工厂的 ``host`` 兜底 (``::`` → ``127.0.0.1``) + 缺 token 时返回 ``None``;
- ``login`` 成功 / NapCat 业务码 ``code != 0`` 失败 / 网络层失败时按候选 host 切换;
- ``set_ob11_config`` 成功 / Not Login 路径;
- ``check_login_status`` 兼容 ``data: bool`` 与 ``data: {isLogin: bool}``;
- ``_authed_call`` 收到 ``Unauthorized`` 自动重 login + 重试一次.

测试**不发任何真实 HTTP**, 全靠 :mod:`httpx` API 的 monkeypatch 替换为 mock.
"""
from __future__ import annotations

# 标准库导入
import hashlib
import json
from pathlib import Path
from typing import Any

# 第三方库导入
import httpx
import pytest

# 项目内模块导入
import src.core.runtime.napcat_webui_client as napcat_client
from src.core.runtime.napcat_webui_client import (
    NapCatWebUIClient,
    NapCatWebUIError,
    _generate_password_hash,
    read_napcat_webui_config,
)


# ==================== Fake httpx.Response ====================
class FakeResponse:
    """最小可用的 :class:`httpx.Response` 替身; 仅暴露被 client 使用的字段."""

    def __init__(self, status_code: int, payload: Any, *, raw_text: str | None = None) -> None:
        self.status_code = status_code
        self._payload = payload
        self._raw_text = raw_text

    @property
    def text(self) -> str:
        if self._raw_text is not None:
            return self._raw_text
        return json.dumps(self._payload, ensure_ascii=False)

    def json(self) -> Any:
        if self._raw_text is not None:
            # 模拟非 JSON 响应
            raise ValueError("not a JSON")
        return self._payload


def _napcat_response(code: int = 0, data: Any = None, message: str = "success") -> FakeResponse:
    """构造 NapCat 业务响应 (status 200 + body.code/message/data)."""
    return FakeResponse(200, {"code": code, "message": message, "data": data})


# ==================== _generate_password_hash ====================
def test_generate_password_hash_matches_upstream_algo() -> None:
    """复现 NapCat ``crypto.createHash('sha256').update(token + '.napcat').digest().hex()``.

    上游实现: ``example/NapCatQQ-main/packages/napcat-webui-backend/src/helper/SignToken.ts:101-103``.
    """
    token = "my-secret-token"
    expected = hashlib.sha256(b"my-secret-token.napcat").hexdigest()
    assert _generate_password_hash(token) == expected


def test_generate_password_hash_is_deterministic() -> None:
    """同一 token 永远得到同一 hash."""
    assert _generate_password_hash("abc") == _generate_password_hash("abc")
    # 不同 token hash 不同
    assert _generate_password_hash("abc") != _generate_password_hash("abd")


# ==================== read_napcat_webui_config ====================
def test_read_napcat_webui_config_returns_none_when_file_missing(tmp_path: Path) -> None:
    """webui.json 不存在时返回 ``None``."""
    napcat_path = tmp_path / "NapCatQQ"
    napcat_path.mkdir()
    assert read_napcat_webui_config(napcat_path) is None


def test_read_napcat_webui_config_returns_none_when_file_corrupt(tmp_path: Path) -> None:
    """webui.json 损坏 (非 JSON) 时返回 ``None``."""
    napcat_path = tmp_path / "NapCatQQ"
    (napcat_path / "config").mkdir(parents=True)
    (napcat_path / "config" / "webui.json").write_text("{invalid json", encoding="utf-8")
    assert read_napcat_webui_config(napcat_path) is None


def test_read_napcat_webui_config_parses_valid_json(tmp_path: Path) -> None:
    """有效 webui.json 应返回 dict."""
    napcat_path = tmp_path / "NapCatQQ"
    (napcat_path / "config").mkdir(parents=True)
    payload = {"host": "127.0.0.1", "port": 6099, "token": "abc"}
    (napcat_path / "config" / "webui.json").write_text(json.dumps(payload), encoding="utf-8")
    assert read_napcat_webui_config(napcat_path) == payload


# ==================== from_napcat_path 工厂 ====================
def test_from_napcat_path_returns_none_when_missing(tmp_path: Path) -> None:
    """webui.json 缺失 → 返回 ``None`` (引导用户先启动 NapCat)."""
    napcat_path = tmp_path / "NapCatQQ"
    napcat_path.mkdir()
    assert NapCatWebUIClient.from_napcat_path(napcat_path) is None


def test_from_napcat_path_returns_none_when_token_empty(tmp_path: Path) -> None:
    """webui.json 缺 token 字段 → 返回 ``None``."""
    napcat_path = tmp_path / "NapCatQQ"
    (napcat_path / "config").mkdir(parents=True)
    (napcat_path / "config" / "webui.json").write_text(
        json.dumps({"host": "127.0.0.1", "port": 6099, "token": ""}), encoding="utf-8"
    )
    assert NapCatWebUIClient.from_napcat_path(napcat_path) is None


def test_from_napcat_path_falls_back_host_when_listen_all(tmp_path: Path) -> None:
    """``host="::"`` (NapCat 默认 listen all) 应映射为 ``"127.0.0.1"``, 让 login 候选探测."""
    napcat_path = tmp_path / "NapCatQQ"
    (napcat_path / "config").mkdir(parents=True)
    (napcat_path / "config" / "webui.json").write_text(
        json.dumps({"host": "::", "port": 6099, "token": "abc"}), encoding="utf-8"
    )
    client = NapCatWebUIClient.from_napcat_path(napcat_path)
    assert client is not None
    assert client._host == "127.0.0.1"
    assert client._port == 6099
    assert client._token == "abc"


def test_from_napcat_path_uses_valid_host_as_is(tmp_path: Path) -> None:
    """正常 host (非 ``::`` / ``""``) 应原样保留."""
    napcat_path = tmp_path / "NapCatQQ"
    (napcat_path / "config").mkdir(parents=True)
    (napcat_path / "config" / "webui.json").write_text(
        json.dumps({"host": "192.168.1.10", "port": 7099, "token": "tk"}), encoding="utf-8"
    )
    client = NapCatWebUIClient.from_napcat_path(napcat_path)
    assert client is not None
    assert client._host == "192.168.1.10"
    assert client._port == 7099


# ==================== login ====================
def test_login_success_returns_credential(monkeypatch: pytest.MonkeyPatch) -> None:
    """正常 login: server 返回 ``{code:0, data:{Credential:...}}``, client 应缓存 credential."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")

    captured: dict[str, Any] = {}

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        captured["url"] = url
        captured["json"] = kwargs.get("json")
        return _napcat_response(code=0, data={"Credential": "cred-xyz"})

    monkeypatch.setattr(httpx, "post", fake_post)

    credential = client.login()

    assert credential == "cred-xyz"
    assert client.credential == "cred-xyz"
    assert captured["url"] == "http://127.0.0.1:6099/api/auth/login"
    # body.hash 应为 sha256(token + ".napcat")
    assert captured["json"]["hash"] == hashlib.sha256(b"abc.napcat").hexdigest()


def test_login_business_error_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """server 返回 code=-1 + message=token invalid 时, client 抛 :class:`NapCatWebUIError`."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        return _napcat_response(code=-1, message="token is invalid", data=None)

    monkeypatch.setattr(httpx, "post", fake_post)

    with pytest.raises(NapCatWebUIError) as exc_info:
        client.login()
    assert "token is invalid" in exc_info.value.message
    assert client.credential is None


def test_login_missing_credential_field_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """server 返回 code=0 但 data 缺 Credential, 应抛错."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        return _napcat_response(code=0, data={})

    monkeypatch.setattr(httpx, "post", fake_post)

    with pytest.raises(NapCatWebUIError, match="Credential"):
        client.login()


def test_login_falls_back_to_next_host_on_network_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """第一个 host 网络失败时, 应自动尝试下一个候选 host 并锁定."""
    # 构造一个 host=127.0.0.1, 让候选列表 ['127.0.0.1', 'localhost']
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")

    calls: list[str] = []

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        calls.append(url)
        if "127.0.0.1" in url:
            raise httpx.RequestError("connection refused", request=httpx.Request("POST", url))
        # localhost 成功
        return _napcat_response(code=0, data={"Credential": "cred-from-localhost"})

    monkeypatch.setattr(httpx, "post", fake_post)

    credential = client.login()

    assert credential == "cred-from-localhost"
    assert client._host == "localhost"  # 锁定到 localhost
    # 应至少调过 127.0.0.1 一次
    assert any("127.0.0.1" in u for u in calls)
    assert any("localhost" in u for u in calls)


def test_login_all_hosts_fail_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """所有候选 host 都连不上时, 抛 :class:`NapCatWebUIError`."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        raise httpx.RequestError("connection refused", request=httpx.Request("POST", url))

    monkeypatch.setattr(httpx, "post", fake_post)

    with pytest.raises(NapCatWebUIError, match="网络层失败"):
        client.login()


# ==================== set_ob11_config ====================
def test_set_ob11_config_success(monkeypatch: pytest.MonkeyPatch) -> None:
    """正常推送 OB11 配置: server 返回 code=0."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "cred-xyz"  # 跳过 lazy login

    captured: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        captured["method"] = method
        captured["url"] = url
        captured["headers"] = kwargs.get("headers", {})
        captured["json"] = kwargs.get("json")
        return _napcat_response(code=0, data=None)

    monkeypatch.setattr(httpx, "request", fake_request)

    config_payload = {"network": {"httpServers": []}, "musicSignUrl": "https://example.com"}
    client.set_ob11_config(config_payload)

    assert captured["method"] == "POST"
    assert captured["url"] == "http://127.0.0.1:6099/api/OB11Config/SetConfig"
    # 关键: body.config 是**字符串化** JSON (NapCat 内部 json5.parse)
    assert "config" in captured["json"]
    assert isinstance(captured["json"]["config"], str)
    assert json.loads(captured["json"]["config"]) == config_payload
    assert captured["headers"]["Authorization"] == "Bearer cred-xyz"


def test_set_ob11_config_not_login_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """server 返回 'Not Login' (QQ 未登录) 时, 抛 NapCatWebUIError 含 'Not Login'."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "cred-xyz"

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        return _napcat_response(code=-1, message="Not Login")

    monkeypatch.setattr(httpx, "request", fake_request)

    with pytest.raises(NapCatWebUIError) as exc_info:
        client.set_ob11_config({"network": {}})
    assert "Not Login" in exc_info.value.message


# ==================== check_login_status ====================
def test_check_login_status_returns_true_when_data_is_bool(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """``data: true`` (NapCat 某些版本返回 bool 直接)."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "cred-xyz"

    monkeypatch.setattr(
        httpx, "request", lambda *args, **kwargs: _napcat_response(code=0, data=True)
    )
    assert client.check_login_status() is True


def test_check_login_status_returns_true_when_data_is_dict_islogin(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """``data: {isLogin: true}`` (NapCat 当前版本)."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "cred-xyz"

    monkeypatch.setattr(
        httpx,
        "request",
        lambda *args, **kwargs: _napcat_response(code=0, data={"isLogin": True}),
    )
    assert client.check_login_status() is True


def test_check_login_status_returns_false_on_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """API 调用失败时返回 ``False`` (吞异常, 不抛)."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "cred-xyz"

    def fake_request(*args: Any, **kwargs: Any) -> FakeResponse:
        return _napcat_response(code=-1, message="some error")

    monkeypatch.setattr(httpx, "request", fake_request)
    assert client.check_login_status() is False


# ==================== _authed_call 401 重试 ====================
def test_authed_call_retries_on_unauthorized(monkeypatch: pytest.MonkeyPatch) -> None:
    """首次请求收到 ``Unauthorized`` 时, client 自动重 login + 重试一次."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    client._credential = "stale-cred"

    call_count: dict[str, int] = {"request": 0, "post": 0}

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        call_count["request"] += 1
        # 首次 (旧 credential) 返回 Unauthorized; 第二次 (重 login 后) 成功
        if call_count["request"] == 1:
            return _napcat_response(code=-1, message="Unauthorized")
        return _napcat_response(code=0, data=None)

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        # login 端点应被重新调用
        call_count["post"] += 1
        return _napcat_response(code=0, data={"Credential": "fresh-cred"})

    monkeypatch.setattr(httpx, "request", fake_request)
    monkeypatch.setattr(httpx, "post", fake_post)

    client.set_ob11_config({"network": {}})

    # request 应被调 2 次 (首次 401, 重试 1 次)
    assert call_count["request"] == 2
    # login 应被调 1 次 (重 login)
    assert call_count["post"] == 1
    # credential 应已更新
    assert client.credential == "fresh-cred"


def test_authed_call_lazy_login_when_no_credential(monkeypatch: pytest.MonkeyPatch) -> None:
    """首次调用 API 时未持 credential, 应先 lazy login."""
    client = NapCatWebUIClient(host="127.0.0.1", port=6099, token="abc")
    assert client.credential is None  # 未 login 状态

    call_count: dict[str, int] = {"request": 0, "post": 0}

    def fake_post(url: str, **kwargs: Any) -> FakeResponse:
        call_count["post"] += 1
        return _napcat_response(code=0, data={"Credential": "cred-on-demand"})

    def fake_request(method: str, url: str, **kwargs: Any) -> FakeResponse:
        call_count["request"] += 1
        return _napcat_response(code=0, data=None)

    monkeypatch.setattr(httpx, "post", fake_post)
    monkeypatch.setattr(httpx, "request", fake_request)

    client.set_ob11_config({"network": {}})

    # 应先 login (post), 再 request
    assert call_count["post"] == 1
    assert call_count["request"] == 1
    assert client.credential == "cred-on-demand"
