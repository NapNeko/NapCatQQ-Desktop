# -*- coding: utf-8 -*-
"""``q_port_probe`` 单测 (mock socket / psutil, 不依赖真实 QQ 进程)."""
from __future__ import annotations

import base64
import json
import socket
from typing import Any
from unittest.mock import MagicMock, patch

import pytest


# ==================== JWT payload 解析 ====================
def _make_jwt(payload: dict[str, Any]) -> str:
    """构造一个 JWT 字符串 (header / signature 随便填, 只解 payload)."""
    header = base64.urlsafe_b64encode(b'{"alg":"HS256"}').rstrip(b"=").decode()
    body = base64.urlsafe_b64encode(json.dumps(payload).encode()).rstrip(b"=").decode()
    return f"{header}.{body}.signature_placeholder"


class TestDecodeJwtPayload:
    def test_decodes_logged_in_payload(self) -> None:
        from src.core.runtime.q_port_probe import _decode_jwt_payload

        token = _make_jwt({"errCode": 0, "uin": "498600841", "uid": "u_xxx"})
        result = _decode_jwt_payload(token)

        assert result is not None
        assert result["errCode"] == 0
        assert result["uin"] == "498600841"

    def test_decodes_not_logged_in_payload(self) -> None:
        """uin 字段缺失 / 空 — payload 仍能解出来, errCode 仍为 0."""
        from src.core.runtime.q_port_probe import _decode_jwt_payload

        token = _make_jwt({"errCode": 0, "uin": ""})
        result = _decode_jwt_payload(token)

        assert result is not None
        assert result["uin"] == ""

    def test_returns_none_for_malformed_token(self) -> None:
        from src.core.runtime.q_port_probe import _decode_jwt_payload

        # 只有两段 (缺 signature), JWT 格式不合法
        assert _decode_jwt_payload("aa.bb") is None
        # base64 解出来不是 JSON
        assert _decode_jwt_payload("aaa.bm90anNvbg.cc") is None  # "notjson"
        # 完全乱码
        assert _decode_jwt_payload("not_a_jwt_at_all") is None


# ==================== 探测请求字节级 ====================
class TestBuildProbePayload:
    def test_payload_uses_noop_link_to_avoid_qq_popup(self) -> None:
        """body 必须用 noop deeplink, 避免 ``tencent://`` 空协议触发 QQ 主窗口弹出."""
        from src.core.runtime.q_port_probe import _build_probe_payload

        result = _build_probe_payload(9210)
        expected = (
            b"POST /tencent HTTP/1.1\r\n"
            b"Host: 127.0.0.1:9210\r\n"
            b"Connection: close\r\n"
            b"Content-Length: 29\r\n"
            b"\r\n"
            b"tencent://snowluma-probe-noop"
        )
        assert result == expected


# ==================== 单端口探测 (mock socket) ====================
class _FakeSocket:
    """mock socket: 模拟 connect/sendall/recv 行为."""

    def __init__(
        self,
        response_chunks: list[bytes] | None = None,
        connect_error: Exception | None = None,
    ) -> None:
        self._response = response_chunks or []
        self._connect_error = connect_error
        self.timeout: float | None = None
        self.sent_data = b""
        self.connected_to: tuple[str, int] | None = None

    def settimeout(self, t: float) -> None:
        self.timeout = t

    def connect(self, addr: tuple[str, int]) -> None:
        if self._connect_error:
            raise self._connect_error
        self.connected_to = addr

    def sendall(self, data: bytes) -> None:
        self.sent_data += data

    def recv(self, _n: int) -> bytes:
        return self._response.pop(0) if self._response else b""

    def __enter__(self) -> "_FakeSocket":
        return self

    def __exit__(self, *_args: Any) -> None:
        return None


class TestProbePort:
    def test_returns_logged_in_info_on_valid_response(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        token = _make_jwt({"errCode": 0, "uin": "498600841", "uid": "u_abc"})
        body = json.dumps({"token": token}).encode()
        response = (
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
            b"\r\n" + body
        )
        fake = _FakeSocket(response_chunks=[response])
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        info = q_port_probe._probe_port(9210, timeout=1.0)

        assert info is not None
        assert info.port == 9210
        assert info.uin == "498600841"
        assert info.uid == "u_abc"
        assert info.logged_in is True

    def test_returns_not_logged_in_when_uin_empty(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        token = _make_jwt({"errCode": 0, "uin": ""})
        response = b"HTTP/1.1 200 OK\r\n\r\n" + token.encode()
        fake = _FakeSocket(response_chunks=[response])
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        info = q_port_probe._probe_port(9210, timeout=1.0)

        assert info is not None
        assert info.uin == ""
        assert info.logged_in is False

    def test_extracts_uin_from_nested_data_field(self, monkeypatch) -> None:
        """部分 QQ 版本把 uin 嵌在 ``data.uin``, 应能兜底取到."""
        from src.core.runtime import q_port_probe

        token = _make_jwt({"errCode": 0, "data": {"uin": "12345678"}})
        response = b"HTTP/1.1 200 OK\r\n\r\n" + token.encode()
        fake = _FakeSocket(response_chunks=[response])
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        info = q_port_probe._probe_port(9210, timeout=1.0)

        assert info is not None
        assert info.uin == "12345678"
        assert info.logged_in is True

    def test_returns_none_on_connection_refused(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        fake = _FakeSocket(connect_error=ConnectionRefusedError())
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        assert q_port_probe._probe_port(9210, timeout=1.0) is None

    def test_returns_none_on_timeout(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        fake = _FakeSocket(connect_error=socket.timeout())
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        assert q_port_probe._probe_port(9210, timeout=1.0) is None

    def test_returns_none_when_response_lacks_jwt(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        fake = _FakeSocket(response_chunks=[b"HTTP/1.1 404 Not Found\r\n\r\n"])
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        assert q_port_probe._probe_port(9210, timeout=1.0) is None

    def test_returns_none_when_errcode_nonzero(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe

        token = _make_jwt({"errCode": 1, "errMsg": "fail"})
        response = b"HTTP/1.1 200 OK\r\n\r\n" + token.encode()
        fake = _FakeSocket(response_chunks=[response])
        monkeypatch.setattr(
            q_port_probe.socket, "socket", lambda *_a, **_k: fake
        )

        assert q_port_probe._probe_port(9210, timeout=1.0) is None


# ==================== probe_qq_login (top-level) ====================
class TestProbeQqLogin:
    def test_invalid_pid_returns_none(self) -> None:
        from src.core.runtime.q_port_probe import probe_qq_login

        assert probe_qq_login(0) is None
        assert probe_qq_login(-1) is None

    def test_uses_psutil_listed_ports_first(self, monkeypatch) -> None:
        """psutil 拿到 PID 的监听端口时, 应优先用那些端口而不是全扫描."""
        from src.core.runtime import q_port_probe
        from src.core.runtime.q_port_probe import probe_qq_login

        monkeypatch.setattr(
            q_port_probe, "_list_listening_ports", lambda _pid: [9215]
        )
        called_ports: list[int] = []

        def fake_probe(port: int, timeout: float) -> Any:
            called_ports.append(port)
            return q_port_probe.QqPortLoginInfo(
                port=port, uin="111", logged_in=True
            )

        monkeypatch.setattr(q_port_probe, "_probe_port", fake_probe)

        info = probe_qq_login(12345)

        assert called_ports == [9215]  # 只探了 psutil 给的那个端口
        assert info is not None
        assert info.uin == "111"

    def test_falls_back_to_full_range_when_psutil_empty(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe
        from src.core.runtime.q_port_probe import probe_qq_login

        monkeypatch.setattr(q_port_probe, "_list_listening_ports", lambda _pid: [])
        called_ports: list[int] = []

        def fake_probe(port: int, _timeout: float) -> Any:
            called_ports.append(port)
            # 直到 9215 才"命中"
            if port == 9215:
                return q_port_probe.QqPortLoginInfo(
                    port=port, uin="222", logged_in=True
                )
            return None

        monkeypatch.setattr(q_port_probe, "_probe_port", fake_probe)

        info = probe_qq_login(12345)

        assert called_ports == [9210, 9211, 9212, 9213, 9214, 9215]
        assert info is not None
        assert info.uin == "222"

    def test_returns_none_when_all_ports_fail(self, monkeypatch) -> None:
        from src.core.runtime import q_port_probe
        from src.core.runtime.q_port_probe import probe_qq_login

        monkeypatch.setattr(q_port_probe, "_list_listening_ports", lambda _pid: [])
        monkeypatch.setattr(q_port_probe, "_probe_port", lambda _p, _t: None)

        assert probe_qq_login(12345) is None
