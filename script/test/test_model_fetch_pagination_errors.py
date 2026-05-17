# -*- coding: utf-8 -*-
"""Tests for pagination and error handling in ModelFetchService.

验证:
- Gemini 分页获取 (nextPageToken, 最多 10 页)
- Anthropic 分页获取 (has_more + last_id, 最多 10 页)
- HTTP 错误处理 (状态码 >= 400)
- 网络超时处理 (30 秒)
- 连接错误分类 (DNS/连接拒绝/TLS)
- JSON 解析失败处理 (记录前 200 字符日志)
- 响应缺少预期字段处理 (记录顶层键列表日志)
"""

import logging

import httpx
import pytest
import respx

from src.core.agent.model_fetch_service import FetchResult, ModelFetchService
from src.core.agent.provider import AzureConfig, ModelEntry, Provider


def _make_provider(
    protocol_type: str = "openai",
    api_base_url: str = "https://api.openai.com/v1",
    api_key_ref: str = "sk-test-key",
    azure_config: AzureConfig | None = None,
) -> Provider:
    """创建测试用 Provider 实例."""
    return Provider(
        provider_id="test-provider",
        name="Test Provider",
        api_base_url=api_base_url,
        api_key_ref=api_key_ref,
        protocol_type=protocol_type,
        azure_config=azure_config,
        models=[
            ModelEntry(model_id="placeholder", max_tokens=4096),
        ],
    )


class TestGeminiPagination:
    """测试 Gemini 分页获取逻辑."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_gemini_single_page_no_token(self):
        """Gemini 单页响应无 nextPageToken 时不继续请求."""
        respx.get("https://generativelanguage.googleapis.com/v1beta/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "models": [
                        {"name": "models/gemini-pro", "displayName": "Gemini Pro"},
                    ]
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="gemini",
            api_base_url="https://generativelanguage.googleapis.com/v1beta",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 1
        assert result.models[0].model_id == "gemini-pro"

    @pytest.mark.asyncio
    @respx.mock
    async def test_gemini_two_pages(self):
        """Gemini 两页分页：第一页有 nextPageToken，第二页无."""
        base_url = "https://generativelanguage.googleapis.com/v1beta/models"

        call_count = 0

        def _side_effect(request: httpx.Request) -> httpx.Response:
            nonlocal call_count
            call_count += 1
            url_str = str(request.url)
            if "pageToken=page2token" in url_str:
                # 第二页
                return httpx.Response(
                    200,
                    json={
                        "models": [
                            {"name": "models/gemini-pro-vision", "displayName": "Gemini Pro Vision"},
                        ]
                    },
                )
            else:
                # 第一页
                return httpx.Response(
                    200,
                    json={
                        "models": [
                            {"name": "models/gemini-pro", "displayName": "Gemini Pro"},
                        ],
                        "nextPageToken": "page2token",
                    },
                )

        respx.get(url__regex=r"https://generativelanguage\.googleapis\.com/v1beta/models.*").mock(
            side_effect=_side_effect
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="gemini",
            api_base_url="https://generativelanguage.googleapis.com/v1beta",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 2
        assert result.models[0].model_id == "gemini-pro"
        assert result.models[1].model_id == "gemini-pro-vision"
        assert call_count == 2

    @pytest.mark.asyncio
    @respx.mock
    async def test_gemini_max_10_pages(self):
        """Gemini 分页最多 10 页后停止."""
        base_url = "https://generativelanguage.googleapis.com/v1beta/models"

        # 所有请求都返回 nextPageToken（模拟无限分页）
        respx.get(url__regex=r"https://generativelanguage\.googleapis\.com/v1beta/models.*").mock(
            return_value=httpx.Response(
                200,
                json={
                    "models": [
                        {"name": "models/model-x", "displayName": "Model X"},
                    ],
                    "nextPageToken": "next-token-always",
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="gemini",
            api_base_url="https://generativelanguage.googleapis.com/v1beta",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        # 最多 10 页，每页 1 个模型
        assert len(result.models) == 10


class TestAnthropicPagination:
    """测试 Anthropic 分页获取逻辑."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_anthropic_single_page_no_more(self):
        """Anthropic 单页响应 has_more=false 时不继续请求."""
        respx.get("https://api.anthropic.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "claude-3-opus-20240229", "type": "model"},
                    ],
                    "has_more": False,
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="anthropic",
            api_base_url="https://api.anthropic.com/v1",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 1
        assert result.models[0].model_id == "claude-3-opus-20240229"

    @pytest.mark.asyncio
    @respx.mock
    async def test_anthropic_two_pages(self):
        """Anthropic 两页分页：第一页 has_more=true，第二页 has_more=false."""
        call_count = 0

        def _side_effect(request: httpx.Request) -> httpx.Response:
            nonlocal call_count
            call_count += 1
            url_str = str(request.url)
            if "after_id=claude-3-opus-20240229" in url_str:
                # 第二页
                return httpx.Response(
                    200,
                    json={
                        "data": [
                            {"id": "claude-3-sonnet-20240229", "type": "model"},
                        ],
                        "has_more": False,
                    },
                )
            else:
                # 第一页
                return httpx.Response(
                    200,
                    json={
                        "data": [
                            {"id": "claude-3-opus-20240229", "type": "model"},
                        ],
                        "has_more": True,
                        "last_id": "claude-3-opus-20240229",
                    },
                )

        respx.get(url__regex=r"https://api\.anthropic\.com/v1/models.*").mock(
            side_effect=_side_effect
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="anthropic",
            api_base_url="https://api.anthropic.com/v1",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 2
        assert result.models[0].model_id == "claude-3-opus-20240229"
        assert result.models[1].model_id == "claude-3-sonnet-20240229"
        assert call_count == 2

    @pytest.mark.asyncio
    @respx.mock
    async def test_anthropic_max_10_pages(self):
        """Anthropic 分页最多 10 页后停止."""
        # 所有请求都返回 has_more=True（模拟无限分页）
        respx.get(url__regex=r"https://api\.anthropic\.com/v1/models.*").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "claude-model", "type": "model"},
                    ],
                    "has_more": True,
                    "last_id": "claude-model",
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="anthropic",
            api_base_url="https://api.anthropic.com/v1",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        # 最多 10 页，每页 1 个模型
        assert len(result.models) == 10

    @pytest.mark.asyncio
    @respx.mock
    async def test_anthropic_stops_when_last_id_missing(self):
        """Anthropic has_more=true 但 last_id 缺失时停止分页."""
        respx.get("https://api.anthropic.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "claude-3-opus", "type": "model"},
                    ],
                    "has_more": True,
                    # 没有 last_id
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="anthropic",
            api_base_url="https://api.anthropic.com/v1",
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 1
        assert result.models[0].model_id == "claude-3-opus"


class TestTimeoutHandling:
    """测试网络超时处理."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_timeout_returns_error(self):
        """请求超时应返回超时错误."""
        respx.get("https://api.openai.com/v1/models").mock(
            side_effect=httpx.ReadTimeout("Read timed out")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "请求超时（30秒）" in result.error_message

    @pytest.mark.asyncio
    @respx.mock
    async def test_connect_timeout_returns_error(self):
        """连接超时应返回超时错误."""
        respx.get("https://api.openai.com/v1/models").mock(
            side_effect=httpx.ConnectTimeout("Connect timed out")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "请求超时（30秒）" in result.error_message


class TestConnectErrorHandling:
    """测试连接错误分类处理."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_connect_error_returns_failure(self):
        """连接错误应返回连接错误信息."""
        respx.get("https://api.openai.com/v1/models").mock(
            side_effect=httpx.ConnectError("Connection refused")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "连接错误:" in result.error_message
        assert "Connection refused" in result.error_message

    @pytest.mark.asyncio
    @respx.mock
    async def test_dns_error_returns_failure(self):
        """DNS 解析失败应返回连接错误."""
        respx.get("https://api.openai.com/v1/models").mock(
            side_effect=httpx.ConnectError(
                "[Errno 11001] getaddrinfo failed"
            )
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "连接错误:" in result.error_message


class TestHttpErrorHandling:
    """测试 HTTP 错误处理."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_http_400_returns_failure(self):
        """HTTP 400 应返回失败结果."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(400, text="Bad Request")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "HTTP 错误 400" in result.error_message

    @pytest.mark.asyncio
    @respx.mock
    async def test_http_500_returns_failure(self):
        """HTTP 500 应返回失败结果."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(500, text="Internal Server Error")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "HTTP 错误 500" in result.error_message

    @pytest.mark.asyncio
    @respx.mock
    async def test_http_error_body_truncated_to_200_chars(self):
        """HTTP 错误响应体应截断到 200 字符."""
        long_body = "x" * 500
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(403, text=long_body)
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "HTTP 错误 403" in result.error_message
        # 错误消息中的响应体部分不超过 200 字符
        body_part = result.error_message.split(": ", 1)[1]
        assert len(body_part) <= 200


class TestJsonParseErrorHandling:
    """测试 JSON 解析失败处理."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_invalid_json_returns_empty_models(self):
        """JSON 解析失败应返回成功结果和空模型列表."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(200, text="not valid json {{{")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []

    @pytest.mark.asyncio
    @respx.mock
    async def test_invalid_json_logs_error(self, caplog):
        """JSON 解析失败应记录错误日志（含前 200 字符）."""
        invalid_response = "this is not json " + "x" * 300
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(200, text=invalid_response)
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")

        with caplog.at_level(logging.ERROR, logger="src.core.agent.model_fetch_service"):
            result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []
        # 验证日志包含供应商名称和响应前 200 字符
        assert "Test Provider" in caplog.text
        assert "JSON 解析失败" in caplog.text


class TestMissingExpectedFieldHandling:
    """测试响应缺少预期字段处理."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_missing_data_field_returns_empty_models(self):
        """OpenAI 响应缺少 data 字段应返回空模型列表."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={"error": "something", "status": "ok"},
            )
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []

    @pytest.mark.asyncio
    @respx.mock
    async def test_missing_data_field_logs_warning(self, caplog):
        """OpenAI 响应缺少 data 字段应记录警告日志（含顶层键列表）."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={"error": "something", "status": "ok"},
            )
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")

        with caplog.at_level(logging.WARNING, logger="src.core.agent.model_fetch_service"):
            result = await service.fetch_models(provider)

        assert result.success is True
        assert "Test Provider" in caplog.text
        assert "data" in caplog.text
        assert "error" in caplog.text
        assert "status" in caplog.text

    @pytest.mark.asyncio
    @respx.mock
    async def test_missing_models_field_gemini_logs_warning(self, caplog):
        """Gemini 响应缺少 models 字段应记录警告日志."""
        respx.get("https://generativelanguage.googleapis.com/v1beta/models").mock(
            return_value=httpx.Response(
                200,
                json={"error": {"message": "invalid"}, "code": 200},
            )
        )

        service = ModelFetchService()
        provider = _make_provider(
            protocol_type="gemini",
            api_base_url="https://generativelanguage.googleapis.com/v1beta",
        )

        with caplog.at_level(logging.WARNING, logger="src.core.agent.model_fetch_service"):
            result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []
        assert "models" in caplog.text

    @pytest.mark.asyncio
    @respx.mock
    async def test_data_field_not_list_logs_warning(self, caplog):
        """data 字段存在但不是列表时应记录警告日志."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={"data": "not a list"},
            )
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")

        with caplog.at_level(logging.WARNING, logger="src.core.agent.model_fetch_service"):
            result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []
        assert "data" in caplog.text
