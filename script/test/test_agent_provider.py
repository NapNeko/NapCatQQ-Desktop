# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/provider.py.

验证 Provider、ModelEntry、ModelConfig 数据模型及 ProviderRegistry 的核心行为。
"""

# 第三方库导入
import pytest
from pydantic import ValidationError as PydanticValidationError

# 项目内模块导入
from src.core.agent.errors import (
    DuplicateProviderError,
    ModelNotFoundError,
    NoActiveProviderError,
)
from src.core.agent.provider import (
    ModelConfig,
    ModelEntry,
    Provider,
    ProviderRegistry,
)


def _make_model_entry(
    model_id: str = "test-model",
    display_name: str = "Test Model",
    max_tokens: int = 4096,
) -> ModelEntry:
    """创建测试用 ModelEntry."""
    return ModelEntry(
        model_id=model_id,
        display_name=display_name,
        max_tokens=max_tokens,
    )


def _make_provider(
    provider_id: str = "test-provider",
    name: str = "Test Provider",
    api_base_url: str = "https://api.example.com/v1",
    api_key_ref: str = "TEST_API_KEY",
    models: list[ModelEntry] | None = None,
) -> Provider:
    """创建测试用 Provider."""
    if models is None:
        models = [_make_model_entry()]
    return Provider(
        provider_id=provider_id,
        name=name,
        api_base_url=api_base_url,
        api_key_ref=api_key_ref,
        models=models,
    )


class TestModelEntry:
    """ModelEntry 模型测试."""

    def test_valid_model_entry(self) -> None:
        entry = ModelEntry(
            model_id="deepseek-chat",
            display_name="DeepSeek Chat",
            max_tokens=8192,
        )
        assert entry.model_id == "deepseek-chat"
        assert entry.display_name == "DeepSeek Chat"
        assert entry.max_tokens == 8192
        assert entry.supports_streaming is True
        assert entry.supports_tools is True

    def test_custom_streaming_and_tools(self) -> None:
        entry = ModelEntry(
            model_id="basic-model",
            display_name="Basic",
            max_tokens=1024,
            supports_streaming=False,
            supports_tools=False,
        )
        assert entry.supports_streaming is False
        assert entry.supports_tools is False

    def test_empty_model_id_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelEntry(model_id="", display_name="X", max_tokens=100)

    def test_max_tokens_zero_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelEntry(model_id="m", display_name="X", max_tokens=0)

    def test_max_tokens_negative_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelEntry(model_id="m", display_name="X", max_tokens=-1)


class TestProvider:
    """Provider 模型测试."""

    def test_valid_provider(self) -> None:
        provider = _make_provider()
        assert provider.provider_id == "test-provider"
        assert provider.name == "Test Provider"
        assert str(provider.api_base_url) == "https://api.example.com/v1"
        assert provider.api_key_ref == "TEST_API_KEY"
        assert len(provider.models) == 1

    def test_empty_provider_id_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            _make_provider(provider_id="")

    def test_empty_name_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            _make_provider(name="")

    def test_empty_api_key_ref_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            _make_provider(api_key_ref="")

    def test_empty_models_list_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            _make_provider(models=[])

    def test_invalid_url_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            _make_provider(api_base_url="not-a-url")

    def test_http_url_accepted(self) -> None:
        provider = _make_provider(api_base_url="http://localhost:11434/v1")
        assert "localhost" in str(provider.api_base_url)

    def test_multiple_models(self) -> None:
        models = [
            _make_model_entry(model_id="model-a", max_tokens=4096),
            _make_model_entry(model_id="model-b", max_tokens=8192),
        ]
        provider = _make_provider(models=models)
        assert len(provider.models) == 2


class TestModelConfig:
    """ModelConfig 模型测试."""

    def test_valid_model_config(self) -> None:
        config = ModelConfig(
            model_id="deepseek-chat",
            provider_id="deepseek",
            max_tokens=4096,
        )
        assert config.model_id == "deepseek-chat"
        assert config.provider_id == "deepseek"
        assert config.temperature == 0.7
        assert config.top_p == 1.0
        assert config.max_tokens == 4096

    def test_custom_temperature(self) -> None:
        config = ModelConfig(
            model_id="m", provider_id="p", max_tokens=100, temperature=1.5
        )
        assert config.temperature == 1.5

    def test_temperature_below_range_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelConfig(
                model_id="m", provider_id="p", max_tokens=100, temperature=-0.1
            )

    def test_temperature_above_range_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelConfig(
                model_id="m", provider_id="p", max_tokens=100, temperature=2.1
            )

    def test_top_p_below_range_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelConfig(
                model_id="m", provider_id="p", max_tokens=100, top_p=-0.01
            )

    def test_top_p_above_range_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelConfig(
                model_id="m", provider_id="p", max_tokens=100, top_p=1.01
            )

    def test_max_tokens_zero_rejected(self) -> None:
        with pytest.raises(PydanticValidationError):
            ModelConfig(model_id="m", provider_id="p", max_tokens=0)

    def test_boundary_temperature_zero(self) -> None:
        config = ModelConfig(
            model_id="m", provider_id="p", max_tokens=1, temperature=0.0
        )
        assert config.temperature == 0.0

    def test_boundary_temperature_two(self) -> None:
        config = ModelConfig(
            model_id="m", provider_id="p", max_tokens=1, temperature=2.0
        )
        assert config.temperature == 2.0


class TestProviderRegistry:
    """ProviderRegistry 测试."""

    def test_register_and_get(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        result = registry.get("test-provider")
        assert result.provider_id == "test-provider"

    def test_register_duplicate_raises(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        with pytest.raises(DuplicateProviderError) as exc_info:
            registry.register(provider)
        assert exc_info.value.provider_id == "test-provider"

    def test_unregister(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.unregister("test-provider")
        with pytest.raises(KeyError):
            registry.get("test-provider")

    def test_unregister_nonexistent_raises(self) -> None:
        registry = ProviderRegistry()
        with pytest.raises(KeyError):
            registry.unregister("nonexistent")

    def test_get_nonexistent_raises(self) -> None:
        registry = ProviderRegistry()
        with pytest.raises(KeyError):
            registry.get("nonexistent")

    def test_list_all_empty(self) -> None:
        registry = ProviderRegistry()
        assert registry.list_all() == []

    def test_list_all_multiple(self) -> None:
        registry = ProviderRegistry()
        p1 = _make_provider(provider_id="p1")
        p2 = _make_provider(provider_id="p2")
        registry.register(p1)
        registry.register(p2)
        result = registry.list_all()
        assert len(result) == 2
        ids = {p.provider_id for p in result}
        assert ids == {"p1", "p2"}

    def test_set_active_and_get_active(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_active("test-provider", "test-model")
        active_provider, active_config = registry.get_active()
        assert active_provider.provider_id == "test-provider"
        assert active_config.model_id == "test-model"
        assert active_config.provider_id == "test-provider"

    def test_get_active_without_setting_raises(self) -> None:
        registry = ProviderRegistry()
        with pytest.raises(NoActiveProviderError):
            registry.get_active()

    def test_set_active_nonexistent_provider_raises(self) -> None:
        registry = ProviderRegistry()
        with pytest.raises(KeyError):
            registry.set_active("nonexistent", "model")

    def test_set_active_nonexistent_model_raises(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        with pytest.raises(ModelNotFoundError) as exc_info:
            registry.set_active("test-provider", "nonexistent-model")
        assert exc_info.value.model_id == "nonexistent-model"
        assert exc_info.value.provider_id == "test-provider"

    def test_set_active_uses_model_max_tokens(self) -> None:
        registry = ProviderRegistry()
        models = [_make_model_entry(model_id="big-model", max_tokens=32000)]
        provider = _make_provider(models=models)
        registry.register(provider)
        registry.set_active("test-provider", "big-model")
        _, config = registry.get_active()
        assert config.max_tokens == 32000

    def test_unregister_active_clears_active(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_active("test-provider", "test-model")
        registry.unregister("test-provider")
        with pytest.raises(NoActiveProviderError):
            registry.get_active()

    def test_set_active_with_multiple_models(self) -> None:
        registry = ProviderRegistry()
        models = [
            _make_model_entry(model_id="model-a", max_tokens=4096),
            _make_model_entry(model_id="model-b", max_tokens=8192),
        ]
        provider = _make_provider(models=models)
        registry.register(provider)

        registry.set_active("test-provider", "model-b")
        _, config = registry.get_active()
        assert config.model_id == "model-b"
        assert config.max_tokens == 8192

    # --- set_enabled tests ---

    def test_set_enabled_disable_provider(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_enabled("test-provider", False)
        result = registry.get("test-provider")
        assert result.enabled is False

    def test_set_enabled_enable_provider(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_enabled("test-provider", False)
        registry.set_enabled("test-provider", True)
        result = registry.get("test-provider")
        assert result.enabled is True

    def test_set_enabled_nonexistent_raises(self) -> None:
        registry = ProviderRegistry()
        with pytest.raises(KeyError):
            registry.set_enabled("nonexistent", False)

    def test_set_enabled_disable_active_clears_active(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_active("test-provider", "test-model")
        registry.set_enabled("test-provider", False)
        with pytest.raises(NoActiveProviderError):
            registry.get_active()

    def test_set_enabled_disable_non_active_keeps_active(self) -> None:
        registry = ProviderRegistry()
        p1 = _make_provider(provider_id="p1")
        p2 = _make_provider(provider_id="p2")
        registry.register(p1)
        registry.register(p2)
        registry.set_active("p1", "test-model")
        registry.set_enabled("p2", False)
        active_provider, _ = registry.get_active()
        assert active_provider.provider_id == "p1"

    def test_set_enabled_enable_does_not_restore_active(self) -> None:
        registry = ProviderRegistry()
        provider = _make_provider()
        registry.register(provider)
        registry.set_active("test-provider", "test-model")
        registry.set_enabled("test-provider", False)
        registry.set_enabled("test-provider", True)
        with pytest.raises(NoActiveProviderError):
            registry.get_active()
