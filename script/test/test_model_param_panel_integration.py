# -*- coding: utf-8 -*-
"""单元测试: ModelParamPanel 集成到 ProviderDetailPanel.

验证:
- ProviderRegistry.update_active_config 正确更新活跃 ModelConfig 字段
- param_changed 信号触发时能正确更新 ModelConfig
- 切换活跃模型时加载对应 ModelConfig

Requirements: 5.4, 5.5
"""
from __future__ import annotations

import pytest

from src.core.agent.errors import NoActiveProviderError
from src.core.agent.provider import ModelConfig, ModelEntry, Provider, ProviderRegistry


class TestUpdateActiveConfig:
    """测试 ProviderRegistry.update_active_config() 方法."""

    def _make_registry_with_active(self) -> ProviderRegistry:
        """创建包含活跃模型的 Registry."""
        registry = ProviderRegistry()
        provider = Provider(
            provider_id="test-provider",
            name="Test Provider",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-test-key",
            models=[
                ModelEntry(model_id="gpt-4o", max_tokens=128000),
                ModelEntry(model_id="gpt-4o-mini", max_tokens=16384),
            ],
        )
        registry.register(provider)
        registry.set_active("test-provider", "gpt-4o")
        return registry

    def test_update_temperature(self) -> None:
        """update_active_config 应正确更新 temperature 字段."""
        registry = self._make_registry_with_active()

        registry.update_active_config(temperature=1.2)

        _, config = registry.get_active()
        assert config.temperature == 1.2
        # 其他字段不变
        assert config.top_p == 1.0
        assert config.max_tokens == 128000

    def test_update_top_p(self) -> None:
        """update_active_config 应正确更新 top_p 字段."""
        registry = self._make_registry_with_active()

        registry.update_active_config(top_p=0.5)

        _, config = registry.get_active()
        assert config.top_p == 0.5
        assert config.temperature == 0.7

    def test_update_max_tokens(self) -> None:
        """update_active_config 应正确更新 max_tokens 字段."""
        registry = self._make_registry_with_active()

        registry.update_active_config(max_tokens=2048)

        _, config = registry.get_active()
        assert config.max_tokens == 2048

    def test_update_multiple_fields(self) -> None:
        """update_active_config 应支持同时更新多个字段."""
        registry = self._make_registry_with_active()

        registry.update_active_config(temperature=0.3, top_p=0.8, max_tokens=1024)

        _, config = registry.get_active()
        assert config.temperature == 0.3
        assert config.top_p == 0.8
        assert config.max_tokens == 1024

    def test_update_preserves_model_and_provider_id(self) -> None:
        """update_active_config 不应改变 model_id 和 provider_id."""
        registry = self._make_registry_with_active()

        registry.update_active_config(temperature=1.5)

        _, config = registry.get_active()
        assert config.model_id == "gpt-4o"
        assert config.provider_id == "test-provider"

    def test_update_no_active_raises_error(self) -> None:
        """无活跃模型时调用 update_active_config 应抛出 NoActiveProviderError."""
        registry = ProviderRegistry()

        with pytest.raises(NoActiveProviderError):
            registry.update_active_config(temperature=1.0)

    def test_update_creates_new_instance(self) -> None:
        """update_active_config 应创建新 ModelConfig 实例 (不可变更新)."""
        registry = self._make_registry_with_active()

        _, original = registry.get_active()
        registry.update_active_config(temperature=1.8)
        _, updated = registry.get_active()

        assert original is not updated
        assert original.temperature == 0.7
        assert updated.temperature == 1.8


class TestSwitchActiveModelLoadsConfig:
    """测试切换活跃模型时加载对应 ModelConfig (Requirement 5.5)."""

    def test_set_active_creates_default_config(self) -> None:
        """set_active 应创建默认 ModelConfig (temperature=0.7, top_p=1.0, max_tokens=模型值)."""
        registry = ProviderRegistry()
        provider = Provider(
            provider_id="p1",
            name="Provider 1",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[
                ModelEntry(model_id="model-a", max_tokens=8192),
                ModelEntry(model_id="model-b", max_tokens=32000),
            ],
        )
        registry.register(provider)

        # 设置第一个模型为活跃
        registry.set_active("p1", "model-a")
        _, config = registry.get_active()
        assert config.model_id == "model-a"
        assert config.temperature == 0.7
        assert config.top_p == 1.0
        assert config.max_tokens == 8192

    def test_switch_active_model_resets_config(self) -> None:
        """切换活跃模型时应加载新模型的默认 ModelConfig."""
        registry = ProviderRegistry()
        provider = Provider(
            provider_id="p1",
            name="Provider 1",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[
                ModelEntry(model_id="model-a", max_tokens=8192),
                ModelEntry(model_id="model-b", max_tokens=32000),
            ],
        )
        registry.register(provider)

        # 设置第一个模型并修改参数
        registry.set_active("p1", "model-a")
        registry.update_active_config(temperature=1.5, top_p=0.3)

        # 切换到第二个模型
        registry.set_active("p1", "model-b")
        _, config = registry.get_active()

        # 应该是新模型的默认配置
        assert config.model_id == "model-b"
        assert config.temperature == 0.7
        assert config.top_p == 1.0
        assert config.max_tokens == 32000

    def test_switch_active_model_uses_new_max_tokens(self) -> None:
        """切换活跃模型时 max_tokens 应使用新模型的 max_tokens 值."""
        registry = ProviderRegistry()
        provider = Provider(
            provider_id="p1",
            name="Provider 1",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[
                ModelEntry(model_id="small-model", max_tokens=4096),
                ModelEntry(model_id="large-model", max_tokens=200000),
            ],
        )
        registry.register(provider)

        registry.set_active("p1", "small-model")
        _, config1 = registry.get_active()
        assert config1.max_tokens == 4096

        registry.set_active("p1", "large-model")
        _, config2 = registry.get_active()
        assert config2.max_tokens == 200000
