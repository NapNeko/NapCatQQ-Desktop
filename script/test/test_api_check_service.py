# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/api_check_service.py.

验证 ApiCheckService 的 HTTP 请求逻辑和信号发射行为。
使用 httpx mock 避免真实网络请求。
"""

from unittest.mock import patch

import httpx
import pytest

from src.core.agent.api_check_service import ApiCheckService


class TestApiCheckServiceLogic:
    """ApiCheckService 核心逻辑测试（不依赖 Qt 事件循环）."""

    def test_success_response(self) -> None:
        """2xx 响应应 emit check_finished(True, '连接成功')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-test-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        mock_response = httpx.Response(200, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        assert results == [(True, "连接成功")]

    def test_401_response(self) -> None:
        """401 响应应 emit check_finished(False, 'API 密钥无效')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "invalid-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        mock_response = httpx.Response(401, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        assert results == [(False, "API 密钥无效")]

    def test_403_response(self) -> None:
        """403 响应应 emit check_finished(False, 'API 密钥无效')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "forbidden-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        mock_response = httpx.Response(403, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        assert results == [(False, "API 密钥无效")]

    def test_timeout_exception(self) -> None:
        """超时应 emit check_finished(False, '连接超时')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-test-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.side_effect = httpx.TimeoutException("Connection timed out")
            service.run()

        assert results == [(False, "连接超时")]

    def test_connect_error(self) -> None:
        """连接错误应 emit check_finished(False, '无法连接到服务器')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-test-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.side_effect = httpx.ConnectError("Connection refused")
            service.run()

        assert results == [(False, "无法连接到服务器")]

    def test_other_http_error(self) -> None:
        """500 等其他 HTTP 错误应 emit check_finished(False, 'HTTP {status_code} 错误')."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-test-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        mock_response = httpx.Response(500, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        assert results == [(False, "HTTP 500 错误")]

    def test_url_trailing_slash_normalized(self) -> None:
        """api_base_url 末尾的斜杠应被正确处理."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1/"
        service._api_key = "sk-test-key"

        results: list[tuple[bool, str]] = []
        service.check_finished.connect(lambda ok, msg: results.append((ok, msg)))

        mock_response = httpx.Response(200, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        # Verify the URL was constructed correctly (no double slash)
        call_args = mock_client.get.call_args
        assert call_args[0][0] == "https://api.example.com/v1/models"
        assert results == [(True, "连接成功")]

    def test_authorization_header(self) -> None:
        """请求应携带正确的 Authorization: Bearer 头."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-my-secret-key"

        mock_response = httpx.Response(200, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        call_args = mock_client.get.call_args
        assert call_args[1]["headers"] == {"Authorization": "Bearer sk-my-secret-key"}

    def test_check_started_emitted(self) -> None:
        """run() 应在开始时 emit check_started 信号."""
        service = ApiCheckService()
        service._api_base_url = "https://api.example.com/v1"
        service._api_key = "sk-test-key"

        started_count: list[int] = [0]
        service.check_started.connect(lambda: started_count.__setitem__(0, started_count[0] + 1))

        mock_response = httpx.Response(200, request=httpx.Request("GET", "https://api.example.com/v1/models"))

        with patch("src.core.agent.api_check_service.httpx.Client") as mock_client_cls:
            mock_client = mock_client_cls.return_value.__enter__.return_value
            mock_client.get.return_value = mock_response
            service.run()

        assert started_count[0] == 1

    def test_start_check_stores_params(self) -> None:
        """start_check 应存储参数."""
        service = ApiCheckService()

        # Patch start() to prevent actual thread execution
        with patch.object(service, "start"):
            service.start_check("https://api.openai.com/v1", "sk-abc123")

        assert service._api_base_url == "https://api.openai.com/v1"
        assert service._api_key == "sk-abc123"
