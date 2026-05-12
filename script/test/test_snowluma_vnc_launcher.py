# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.vnc_launcher` 单测 (W10).

覆盖:

- ``build_snowluma_novnc_url``: URL 各 query 参数正确性 + 空密码 raise
- ``read_remote_vnc_password``: 成功 / 空文件 / 远端错误 3 路径
- ``open_url_in_default_browser``: webbrowser 异常降级
- ``open_snowluma_vnc``: 整合三步 + 失败路径
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock
from urllib.parse import parse_qs, urlparse

import pytest

from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import RemoteCommandResult
from src.core.remote.snowluma import (
    SnowLumaRemotePaths,
    SnowLumaTunnelEndpoint,
    build_snowluma_novnc_url,
    open_snowluma_vnc,
    read_remote_vnc_password,
)
from src.core.remote.snowluma import vnc_launcher as vnc_mod


# ==================== Fake backend ====================
class FakeBackend(ExecutionBackend):
    def __init__(self) -> None:
        self._stdout: str = ""
        self._exit: int = 0

    def set_response(self, stdout: str, exit_status: int = 0) -> None:
        self._stdout = stdout
        self._exit = exit_status

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        return RemoteCommandResult(
            command=command, exit_status=self._exit, stdout=self._stdout
        )

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        return RemoteCommandResult(command="", exit_status=0)

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        pass

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        pass


@pytest.fixture
def backend() -> FakeBackend:
    return FakeBackend()


@pytest.fixture
def paths() -> SnowLumaRemotePaths:
    return SnowLumaRemotePaths.from_base("/opt/sl")


@pytest.fixture
def novnc_endpoint() -> SnowLumaTunnelEndpoint:
    return SnowLumaTunnelEndpoint(label="novnc", local_port=47609, remote_port=6081)


# ==================== build_snowluma_novnc_url ====================
class TestBuildUrl:
    def test_default_query_params(
        self, novnc_endpoint: SnowLumaTunnelEndpoint
    ) -> None:
        url = build_snowluma_novnc_url(novnc_endpoint, vnc_password="abc123")
        parsed = urlparse(url)
        assert parsed.scheme == "http"
        assert parsed.netloc == "127.0.0.1:47609"
        assert parsed.path == "/vnc.html"
        params = parse_qs(parsed.query)
        assert params["autoconnect"] == ["1"]
        assert params["resize"] == ["scale"]
        assert params["password"] == ["abc123"]
        assert params["view_only"] == ["0"]
        assert params["reconnect"] == ["1"]

    def test_view_only_true(
        self, novnc_endpoint: SnowLumaTunnelEndpoint
    ) -> None:
        url = build_snowluma_novnc_url(
            novnc_endpoint, vnc_password="x", view_only=True
        )
        params = parse_qs(urlparse(url).query)
        assert params["view_only"] == ["1"]

    def test_autoconnect_false(
        self, novnc_endpoint: SnowLumaTunnelEndpoint
    ) -> None:
        url = build_snowluma_novnc_url(
            novnc_endpoint, vnc_password="x", autoconnect=False
        )
        params = parse_qs(urlparse(url).query)
        assert params["autoconnect"] == ["0"]

    def test_empty_password_raises(
        self, novnc_endpoint: SnowLumaTunnelEndpoint
    ) -> None:
        with pytest.raises(ValueError, match="vnc_password 必须非空"):
            build_snowluma_novnc_url(novnc_endpoint, vnc_password="")

    def test_password_with_special_chars_url_encoded(
        self, novnc_endpoint: SnowLumaTunnelEndpoint
    ) -> None:
        """密码含 ``&`` / ``=`` 等 URL 保留字符时应被 percent-encode."""
        url = build_snowluma_novnc_url(
            novnc_endpoint, vnc_password="a&b=c"
        )
        params = parse_qs(urlparse(url).query)
        # parse_qs 自动 URL-decode, 得到原文
        assert params["password"] == ["a&b=c"]
        # URL 中应有 percent-encoded 形式
        assert "%26" in url or "&amp;" in url or "a%26b" in url


# ==================== read_remote_vnc_password ====================
class TestReadRemoteVncPassword:
    def test_success(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.set_response("abc123def\n", exit_status=0)
        assert read_remote_vnc_password(backend, paths) == "abc123def"

    def test_empty_file_raises(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.set_response("", exit_status=0)
        with pytest.raises(RuntimeError, match="密码文件为空"):
            read_remote_vnc_password(backend, paths)

    def test_whitespace_only_raises(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.set_response("   \n\n", exit_status=0)
        with pytest.raises(RuntimeError, match="密码文件为空"):
            read_remote_vnc_password(backend, paths)

    def test_remote_command_failure_raises(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.set_response("", exit_status=1)
        with pytest.raises(RuntimeError, match="读取远端 VNC 密码失败"):
            read_remote_vnc_password(backend, paths)


# ==================== open_snowluma_vnc 集成 ====================
class TestOpenSnowlumaVnc:
    def test_success_path(
        self,
        backend: FakeBackend,
        paths: SnowLumaRemotePaths,
        novnc_endpoint: SnowLumaTunnelEndpoint,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        backend.set_response("secret_pwd\n", exit_status=0)
        opened_urls: list[str] = []
        monkeypatch.setattr(
            vnc_mod, "open_url_in_default_browser", lambda url: (opened_urls.append(url), True)[1]
        )
        ok, message = open_snowluma_vnc(backend, paths, novnc_endpoint)
        assert ok
        # 浏览器收到的 URL 含明文密码 (送给系统浏览器, 但不进 Python 返回值)
        assert opened_urls
        assert "secret_pwd" in opened_urls[0]
        assert "127.0.0.1:47609" in opened_urls[0]
        # P10 (review): 返回的 message **不含密码**, 只是脱敏端点
        assert "secret_pwd" not in message
        assert "password" not in message.lower()
        # 但包含端口便于 UI 提示用户
        assert "47609" in message

    def test_success_returns_sanitized_endpoint_not_url(
        self,
        backend: FakeBackend,
        paths: SnowLumaRemotePaths,
        novnc_endpoint: SnowLumaTunnelEndpoint,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """P10 review 回归: 即使密码含特殊字符, 返回值绝不暴露."""
        backend.set_response("a&b=c==!\n", exit_status=0)
        monkeypatch.setattr(vnc_mod, "open_url_in_default_browser", lambda url: True)
        ok, message = open_snowluma_vnc(backend, paths, novnc_endpoint)
        assert ok
        assert "a&b" not in message
        assert "==!" not in message
        assert "%" not in message  # 任何 URL-encoded 形式也不能出现

    def test_missing_password_returns_error(
        self,
        backend: FakeBackend,
        paths: SnowLumaRemotePaths,
        novnc_endpoint: SnowLumaTunnelEndpoint,
    ) -> None:
        backend.set_response("", exit_status=0)
        ok, message = open_snowluma_vnc(backend, paths, novnc_endpoint)
        assert not ok
        assert "密码文件为空" in message

    def test_browser_open_failure_returns_error(
        self,
        backend: FakeBackend,
        paths: SnowLumaRemotePaths,
        novnc_endpoint: SnowLumaTunnelEndpoint,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        backend.set_response("pwd\n", exit_status=0)
        monkeypatch.setattr(
            vnc_mod, "open_url_in_default_browser", lambda url: False
        )
        ok, message = open_snowluma_vnc(backend, paths, novnc_endpoint)
        assert not ok
        assert "浏览器" in message


# ==================== open_url_in_default_browser 异常容错 ====================
class TestOpenInBrowser:
    def test_webbrowser_exception_returns_false(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        def _raise(*args: object, **kwargs: object) -> bool:
            raise RuntimeError("no display")

        monkeypatch.setattr(vnc_mod.webbrowser, "open", _raise)
        assert vnc_mod.open_url_in_default_browser("http://x") is False
