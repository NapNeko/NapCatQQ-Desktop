# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/model_fetch_service.py.

验证 ModelFetchService 核心逻辑：URL 构建、认证头构建、API 密钥检查、
模型列表获取和 ModelEntry 转换。
"""

import pytest
import httpx
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


class TestBuildFetchUrl:
    """测试 _build_fetch_url 方法."""

    def test_openai_url(self):
        """OpenAI 协议应构建 {api_base_url}/models."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        url = service._build_fetch_url(provider)
        assert url == "https://api.openai.com/v1/models"

    def test_openai_url_strips_trailing_slash(self):
        """应去除 api_base_url 尾部斜杠."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1/")
        url = service._build_fetch_url(provider)
        assert url == "https://api.openai.com/v1/models"

    def test_anthropic_url(self):
        """Anthropic 协议应构建 {api_base_url}/models."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="anthropic", api_base_url="https://api.anthropic.com/v1")
        url = service._build_fetch_url(provider)
        assert url == "https://api.anthropic.com/v1/models"

    def test_gemini_url(self):
        """Gemini 协议应构建 {api_base_url}/models."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="gemini", api_base_url="https://generativelanguage.googleapis.com/v1beta")
        url = service._build_fetch_url(provider)
        assert url == "https://generativelanguage.googleapis.com/v1beta/models"

    def test_azure_url(self):
        """Azure 协议应构建 {resource_endpoint}/openai/models?api-version={api_version}."""
        service = ModelFetchService()
        azure_cfg = AzureConfig(
            resource_endpoint="https://myresource.openai.azure.com",
            deployment_name="gpt-4",
            api_version="2024-02-01",
        )
        provider = _make_provider(
            protocol_type="azure",
            api_base_url="https://placeholder.com",
            azure_config=azure_cfg,
        )
        url = service._build_fetch_url(provider)
        assert url == "https://myresource.openai.azure.com/openai/models?api-version=2024-02-01"

    def test_azure_url_strips_trailing_slash(self):
        """Azure resource_endpoint 尾部斜杠应被去除."""
        service = ModelFetchService()
        azure_cfg = AzureConfig(
            resource_endpoint="https://myresource.openai.azure.com/",
            deployment_name="gpt-4",
            api_version="2024-02-01",
        )
        provider = _make_provider(
            protocol_type="azure",
            api_base_url="https://placeholder.com",
            azure_config=azure_cfg,
        )
        url = service._build_fetch_url(provider)
        assert url == "https://myresource.openai.azure.com/openai/models?api-version=2024-02-01"


class TestBuildFetchHeaders:
    """测试 _build_fetch_headers 方法."""

    def test_openai_headers(self):
        """OpenAI 应使用 Bearer token 认证."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_key_ref="sk-abc123")
        headers = service._build_fetch_headers(provider)
        assert headers == {"Authorization": "Bearer sk-abc123"}

    def test_anthropic_headers(self):
        """Anthropic 应使用 x-api-key 和 anthropic-version."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="anthropic", api_key_ref="sk-ant-key")
        headers = service._build_fetch_headers(provider)
        assert headers == {
            "x-api-key": "sk-ant-key",
            "anthropic-version": "2023-06-01",
        }

    def test_gemini_headers(self):
        """Gemini 应使用 x-goog-api-key."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="gemini", api_key_ref="AIza-gemini-key")
        headers = service._build_fetch_headers(provider)
        assert headers == {"x-goog-api-key": "AIza-gemini-key"}

    def test_azure_headers(self):
        """Azure 应使用 api-key."""
        service = ModelFetchService()
        provider = _make_provider(protocol_type="azure", api_key_ref="azure-key-123")
        headers = service._build_fetch_headers(provider)
        assert headers == {"api-key": "azure-key-123"}


class TestApiKeyCheck:
    """测试 API 密钥未配置检查."""

    @pytest.mark.asyncio
    async def test_empty_api_key_returns_error(self):
        """api_key_ref 为空时应立即返回错误."""
        service = ModelFetchService()
        # Provider 的 api_key_ref 有 min_length=1 约束，所以我们需要绕过
        # 直接测试 fetch_models 中的逻辑
        provider = _make_provider(api_key_ref="sk-test")
        # 通过 model_copy 设置空 api_key_ref (绕过 pydantic 验证)
        provider_dict = provider.model_dump()
        provider_dict["api_key_ref"] = ""
        # 使用 model_construct 绕过验证
        empty_key_provider = Provider.model_construct(**provider_dict)

        result = await service.fetch_models(empty_key_provider)
        assert result.success is False
        assert "API 密钥未配置" in result.error_message

    @pytest.mark.asyncio
    async def test_whitespace_api_key_returns_error(self):
        """api_key_ref 仅含空白时应立即返回错误."""
        service = ModelFetchService()
        provider_dict = _make_provider(api_key_ref="sk-test").model_dump()
        provider_dict["api_key_ref"] = "   "
        empty_key_provider = Provider.model_construct(**provider_dict)

        result = await service.fetch_models(empty_key_provider)
        assert result.success is False
        assert "API 密钥未配置" in result.error_message


class TestFetchModelsIntegration:
    """测试 fetch_models 完整流程 (使用 respx mock)."""

    @pytest.mark.asyncio
    @respx.mock
    async def test_openai_fetch_success(self):
        """OpenAI 协议成功获取模型列表."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "gpt-4", "object": "model"},
                        {"id": "gpt-3.5-turbo", "object": "model"},
                    ]
                },
            )
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 2
        assert result.models[0].model_id == "gpt-4"
        assert result.models[1].model_id == "gpt-3.5-turbo"

    @pytest.mark.asyncio
    @respx.mock
    async def test_anthropic_fetch_success(self):
        """Anthropic 协议成功获取模型列表."""
        respx.get("https://api.anthropic.com/v1/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "claude-3-opus-20240229", "type": "model"},
                        {"id": "claude-3-sonnet-20240229", "type": "model"},
                    ]
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
        assert len(result.models) == 2
        assert result.models[0].model_id == "claude-3-opus-20240229"

    @pytest.mark.asyncio
    @respx.mock
    async def test_gemini_fetch_success(self):
        """Gemini 协议成功获取模型列表（去除 models/ 前缀）."""
        respx.get("https://generativelanguage.googleapis.com/v1beta/models").mock(
            return_value=httpx.Response(
                200,
                json={
                    "models": [
                        {"name": "models/gemini-pro", "displayName": "Gemini Pro"},
                        {"name": "models/gemini-pro-vision", "displayName": "Gemini Pro Vision"},
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
        assert len(result.models) == 2
        assert result.models[0].model_id == "gemini-pro"
        assert result.models[1].model_id == "gemini-pro-vision"

    @pytest.mark.asyncio
    @respx.mock
    async def test_azure_fetch_success(self):
        """Azure 协议成功获取模型列表."""
        respx.get("https://myresource.openai.azure.com/openai/models?api-version=2024-02-01").mock(
            return_value=httpx.Response(
                200,
                json={
                    "data": [
                        {"id": "gpt-4", "object": "model"},
                    ]
                },
            )
        )

        service = ModelFetchService()
        azure_cfg = AzureConfig(
            resource_endpoint="https://myresource.openai.azure.com",
            deployment_name="gpt-4",
            api_version="2024-02-01",
        )
        provider = _make_provider(
            protocol_type="azure",
            api_base_url="https://placeholder.com",
            azure_config=azure_cfg,
        )
        result = await service.fetch_models(provider)

        assert result.success is True
        assert len(result.models) == 1
        assert result.models[0].model_id == "gpt-4"

    @pytest.mark.asyncio
    @respx.mock
    async def test_http_error_returns_failure(self):
        """HTTP 错误应返回失败结果."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(401, text="Unauthorized")
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is False
        assert "401" in result.error_message

    @pytest.mark.asyncio
    @respx.mock
    async def test_empty_model_list_returns_success(self):
        """空模型列表应返回成功结果和空列表."""
        respx.get("https://api.openai.com/v1/models").mock(
            return_value=httpx.Response(200, json={"data": []})
        )

        service = ModelFetchService()
        provider = _make_provider(protocol_type="openai", api_base_url="https://api.openai.com/v1")
        result = await service.fetch_models(provider)

        assert result.success is True
        assert result.models == []


class TestBuildModelEntries:
    """测试 _build_model_entries 方法."""

    def test_basic_model_entry_creation(self):
        """应正确创建 ModelEntry 并推断能力."""
        service = ModelFetchService()
        entries = service._build_model_entries(["gpt-4o", "text-embedding-ada-002"])

        assert len(entries) == 2

        # gpt-4o: supports_vision=True, supports_tools=True
        gpt4o = entries[0]
        assert gpt4o.model_id == "gpt-4o"
        assert gpt4o.max_tokens == 4096
        assert gpt4o.supports_vision is True
        assert gpt4o.supports_tools is True

        # text-embedding-ada-002: supports_embedding=True, supports_tools=False
        embed = entries[1]
        assert embed.model_id == "text-embedding-ada-002"
        assert embed.supports_embedding is True
        assert embed.supports_tools is False

    def test_group_name_parsed(self):
        """应正确解析 group_name."""
        service = ModelFetchService()
        entries = service._build_model_entries(["gpt-4-turbo-2024-04-09"])

        assert entries[0].group_name == "gpt"

    def test_display_name_equals_model_id(self):
        """display_name 应等于 model_id."""
        service = ModelFetchService()
        entries = service._build_model_entries(["claude-3-opus-20240229"])

        assert entries[0].display_name == "claude-3-opus-20240229"


class TestMergeWithExisting:
    """测试 merge_with_existing 方法 — 重新获取时保留手动覆盖."""

    def test_new_models_use_heuristic_inference(self):
        """新模型应使用启发式推断创建 ModelEntry."""
        service = ModelFetchService()
        result = service.merge_with_existing(
            fetched_model_ids=["gpt-4o", "claude-3-opus-20240229"],
            existing_models=[],
        )

        assert len(result) == 2
        assert result[0].model_id == "gpt-4o"
        assert result[0].max_tokens == 4096
        assert result[0].supports_vision is True
        assert result[1].model_id == "claude-3-opus-20240229"
        assert result[1].supports_vision is True

    def test_existing_models_preserve_manual_overrides(self):
        """已存在的模型应保留手动覆盖的能力标志."""
        service = ModelFetchService()

        # 用户手动将 gpt-4o 的 supports_vision 设为 False（覆盖推断值 True）
        existing = [
            ModelEntry(
                model_id="gpt-4o",
                display_name="GPT-4o (custom)",
                group_name="gpt",
                max_tokens=8192,
                supports_vision=False,  # 手动覆盖: 推断应为 True
                supports_tools=False,   # 手动覆盖: 推断应为 True
                supports_reasoning=True,  # 手动覆盖: 推断应为 False
            ),
        ]

        result = service.merge_with_existing(
            fetched_model_ids=["gpt-4o"],
            existing_models=existing,
        )

        assert len(result) == 1
        merged_model = result[0]
        # 所有字段应保留原值，不被推断覆盖
        assert merged_model.model_id == "gpt-4o"
        assert merged_model.display_name == "GPT-4o (custom)"
        assert merged_model.max_tokens == 8192
        assert merged_model.supports_vision is False
        assert merged_model.supports_tools is False
        assert merged_model.supports_reasoning is True

    def test_mixed_new_and_existing_models(self):
        """混合场景：已有模型保留覆盖，新模型使用推断."""
        service = ModelFetchService()

        existing = [
            ModelEntry(
                model_id="gpt-4o",
                display_name="My GPT-4o",
                group_name="gpt",
                max_tokens=16384,
                supports_vision=False,  # 手动覆盖
            ),
        ]

        result = service.merge_with_existing(
            fetched_model_ids=["gpt-4o", "claude-3-sonnet-20240229"],
            existing_models=existing,
        )

        assert len(result) == 2

        # gpt-4o: 保留已有
        assert result[0].model_id == "gpt-4o"
        assert result[0].display_name == "My GPT-4o"
        assert result[0].max_tokens == 16384
        assert result[0].supports_vision is False

        # claude-3-sonnet: 新模型，使用推断
        assert result[1].model_id == "claude-3-sonnet-20240229"
        assert result[1].max_tokens == 4096
        assert result[1].supports_vision is True  # claude-3 → vision=True

    def test_existing_model_not_in_fetched_list_is_dropped(self):
        """已有模型如果不在获取列表中，不会出现在结果中."""
        service = ModelFetchService()

        existing = [
            ModelEntry(
                model_id="old-model",
                max_tokens=2048,
            ),
        ]

        result = service.merge_with_existing(
            fetched_model_ids=["new-model"],
            existing_models=existing,
        )

        assert len(result) == 1
        assert result[0].model_id == "new-model"

    def test_empty_fetched_list_returns_empty(self):
        """获取列表为空时返回空列表."""
        service = ModelFetchService()

        existing = [
            ModelEntry(model_id="gpt-4o", max_tokens=4096),
        ]

        result = service.merge_with_existing(
            fetched_model_ids=[],
            existing_models=existing,
        )

        assert result == []

    def test_preserves_order_of_fetched_list(self):
        """结果列表应保持获取列表的顺序."""
        service = ModelFetchService()

        existing = [
            ModelEntry(model_id="model-b", max_tokens=4096),
            ModelEntry(model_id="model-a", max_tokens=4096),
        ]

        result = service.merge_with_existing(
            fetched_model_ids=["model-a", "model-b", "model-c"],
            existing_models=existing,
        )

        assert [m.model_id for m in result] == ["model-a", "model-b", "model-c"]

    def test_new_model_group_name_parsed(self):
        """新模型应正确解析 group_name."""
        service = ModelFetchService()

        result = service.merge_with_existing(
            fetched_model_ids=["gpt-4-turbo-2024-04-09"],
            existing_models=[],
        )

        assert result[0].group_name == "gpt"

    def test_new_model_display_name_equals_model_id(self):
        """新模型的 display_name 应等于 model_id."""
        service = ModelFetchService()

        result = service.merge_with_existing(
            fetched_model_ids=["claude-3-opus-20240229"],
            existing_models=[],
        )

        assert result[0].display_name == "claude-3-opus-20240229"
