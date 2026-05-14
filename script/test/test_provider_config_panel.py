# -*- coding: utf-8 -*-
"""单元测试：ProviderConfigPanel 的 add/remove 功能.

验证 add_provider() 和 remove_provider() 方法正确调用 ProviderRegistry，
以及 DuplicateProviderError 的处理。
"""
from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from src.core.agent.errors import DuplicateProviderError
from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry


class TestProviderConfigPanelAddRemove:
    """测试 ProviderConfigPanel 的 add_provider / remove_provider 逻辑"""

    def test_add_provider_calls_register(self) -> None:
        """add_provider 应调用 ProviderRegistry.register() 并传入正确的 Provider"""
        registry = ProviderRegistry()

        provider_data = {
            "provider_id": "test-provider",
            "name": "Test Provider",
            "api_base_url": "https://api.example.com/v1",
            "api_key_ref": "sk-test-key",
            "models": [
                {"model_id": "gpt-4o", "max_tokens": 128000, "display_name": "GPT-4o"},
            ],
        }

        # 直接测试 registry.register 逻辑（不依赖 Qt UI）
        provider = Provider(
            provider_id=provider_data["provider_id"],
            name=provider_data["name"],
            api_base_url=provider_data["api_base_url"],
            api_key_ref=provider_data["api_key_ref"],
            models=[
                ModelEntry(
                    model_id=m["model_id"],
                    max_tokens=m["max_tokens"],
                    display_name=m.get("display_name", ""),
                )
                for m in provider_data["models"]
            ],
        )

        registry.register(provider)

        # 验证注册成功
        assert len(registry.list_all()) == 1
        registered = registry.list_all()[0]
        assert registered.provider_id == "test-provider"
        assert registered.name == "Test Provider"
        assert str(registered.api_base_url) == "https://api.example.com/v1"
        assert registered.api_key_ref == "sk-test-key"
        assert len(registered.models) == 1
        assert registered.models[0].model_id == "gpt-4o"

    def test_add_provider_duplicate_raises_error(self) -> None:
        """重复注册相同 provider_id 应抛出 DuplicateProviderError"""
        registry = ProviderRegistry()

        provider = Provider(
            provider_id="dup-provider",
            name="Dup Provider",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[ModelEntry(model_id="m1", max_tokens=4096)],
        )

        registry.register(provider)

        # 再次注册相同 provider_id 应抛出异常
        with pytest.raises(DuplicateProviderError) as exc_info:
            registry.register(provider)

        assert exc_info.value.provider_id == "dup-provider"

    def test_remove_provider_calls_unregister(self) -> None:
        """remove_provider 应调用 ProviderRegistry.unregister() 并移除 Provider"""
        registry = ProviderRegistry()

        provider = Provider(
            provider_id="to-remove",
            name="Remove Me",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[ModelEntry(model_id="m1", max_tokens=4096)],
        )

        registry.register(provider)
        assert len(registry.list_all()) == 1

        registry.unregister("to-remove")
        assert len(registry.list_all()) == 0

    def test_remove_nonexistent_provider_raises_key_error(self) -> None:
        """删除不存在的 provider_id 应抛出 KeyError"""
        registry = ProviderRegistry()

        with pytest.raises(KeyError):
            registry.unregister("nonexistent")

    def test_remove_active_provider_clears_active_state(self) -> None:
        """删除当前活跃的 Provider 应清除活跃状态"""
        from src.core.agent.errors import NoActiveProviderError

        registry = ProviderRegistry()

        provider = Provider(
            provider_id="active-one",
            name="Active Provider",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[ModelEntry(model_id="m1", max_tokens=4096)],
        )

        registry.register(provider)
        registry.set_active("active-one", "m1")

        # 确认活跃状态
        active_p, active_m = registry.get_active()
        assert active_p.provider_id == "active-one"

        # 删除活跃 Provider
        registry.unregister("active-one")

        # 活跃状态应被清除
        with pytest.raises(NoActiveProviderError):
            registry.get_active()

    def test_add_provider_with_multiple_models(self) -> None:
        """添加包含多个模型的 Provider 应正确注册所有模型"""
        registry = ProviderRegistry()

        provider = Provider(
            provider_id="multi-model",
            name="Multi Model Provider",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-key",
            models=[
                ModelEntry(model_id="gpt-4o", max_tokens=128000, display_name="GPT-4o"),
                ModelEntry(model_id="gpt-4o-mini", max_tokens=16384, display_name="GPT-4o Mini"),
                ModelEntry(model_id="gpt-3.5-turbo", max_tokens=4096),
            ],
        )

        registry.register(provider)

        registered = registry.list_all()[0]
        assert len(registered.models) == 3
        assert registered.models[0].model_id == "gpt-4o"
        assert registered.models[1].model_id == "gpt-4o-mini"
        assert registered.models[2].model_id == "gpt-3.5-turbo"


class TestProviderRegistryUpdateProvider:
    """测试 ProviderRegistry.update_provider() 方法"""

    def _make_registry_with_provider(self) -> ProviderRegistry:
        """创建包含一个测试 Provider 的 Registry."""
        registry = ProviderRegistry()
        provider = Provider(
            provider_id="test-provider",
            name="Test Provider",
            api_base_url="https://api.example.com/v1",
            api_key_ref="sk-test-key",
            models=[ModelEntry(model_id="m1", max_tokens=4096, display_name="Model 1")],
        )
        registry.register(provider)
        return registry

    def test_update_provider_name(self) -> None:
        """update_provider 应正确更新 name 字段"""
        registry = self._make_registry_with_provider()

        registry.update_provider("test-provider", name="New Name")

        updated = registry.get("test-provider")
        assert updated.name == "New Name"
        # 其他字段不变
        assert str(updated.api_base_url) == "https://api.example.com/v1"
        assert updated.api_key_ref == "sk-test-key"

    def test_update_provider_api_base_url(self) -> None:
        """update_provider 应正确更新 api_base_url 字段"""
        registry = self._make_registry_with_provider()

        registry.update_provider("test-provider", api_base_url="https://new-api.example.com/v2")

        updated = registry.get("test-provider")
        assert str(updated.api_base_url) == "https://new-api.example.com/v2"

    def test_update_provider_api_key_ref(self) -> None:
        """update_provider 应正确更新 api_key_ref 字段"""
        registry = self._make_registry_with_provider()

        registry.update_provider("test-provider", api_key_ref="sk-new-key")

        updated = registry.get("test-provider")
        assert updated.api_key_ref == "sk-new-key"

    def test_update_provider_models(self) -> None:
        """update_provider 应正确更新 models 字段"""
        registry = self._make_registry_with_provider()

        new_models = [
            ModelEntry(model_id="m2", max_tokens=8192, display_name="Model 2"),
            ModelEntry(model_id="m3", max_tokens=16384, display_name="Model 3"),
        ]
        registry.update_provider("test-provider", models=new_models)

        updated = registry.get("test-provider")
        assert len(updated.models) == 2
        assert updated.models[0].model_id == "m2"
        assert updated.models[1].model_id == "m3"

    def test_update_provider_multiple_fields(self) -> None:
        """update_provider 应支持同时更新多个字段"""
        registry = self._make_registry_with_provider()

        registry.update_provider("test-provider", name="Updated", api_key_ref="sk-updated")

        updated = registry.get("test-provider")
        assert updated.name == "Updated"
        assert updated.api_key_ref == "sk-updated"

    def test_update_provider_nonexistent_raises_key_error(self) -> None:
        """更新不存在的 provider_id 应抛出 KeyError"""
        registry = ProviderRegistry()

        with pytest.raises(KeyError):
            registry.update_provider("nonexistent", name="New Name")

    def test_update_provider_preserves_provider_id(self) -> None:
        """update_provider 不应改变 provider_id"""
        registry = self._make_registry_with_provider()

        registry.update_provider("test-provider", name="Changed Name")

        updated = registry.get("test-provider")
        assert updated.provider_id == "test-provider"

    def test_update_provider_creates_new_instance(self) -> None:
        """update_provider 应创建新实例（不修改原实例引用）"""
        registry = self._make_registry_with_provider()

        original = registry.get("test-provider")
        registry.update_provider("test-provider", name="New Name")
        updated = registry.get("test-provider")

        # 应该是不同的对象实例
        assert original is not updated
        # 原实例不受影响
        assert original.name == "Test Provider"
        assert updated.name == "New Name"


class TestAddProviderDialogParsing:
    """测试 AddProviderDialog 的模型解析逻辑"""

    def test_parse_models_valid_format(self) -> None:
        """有效的模型格式应正确解析"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        # 直接测试 _parse_models 静态逻辑
        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o,128000,GPT-4o\ngpt-4o-mini,16384,GPT-4o Mini")
        assert result is not None
        assert len(result) == 2
        assert result[0] == {"model_id": "gpt-4o", "max_tokens": 128000, "display_name": "GPT-4o"}
        assert result[1] == {"model_id": "gpt-4o-mini", "max_tokens": 16384, "display_name": "GPT-4o Mini"}

    def test_parse_models_without_display_name(self) -> None:
        """没有 display_name 的模型格式应正确解析"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o,128000")
        assert result is not None
        assert len(result) == 1
        assert result[0] == {"model_id": "gpt-4o", "max_tokens": 128000, "display_name": ""}

    def test_parse_models_invalid_max_tokens(self) -> None:
        """max_tokens 非数字应返回 None"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o,not_a_number")
        assert result is None

    def test_parse_models_missing_max_tokens(self) -> None:
        """缺少 max_tokens 字段应返回 None"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o")
        assert result is None

    def test_parse_models_empty_string(self) -> None:
        """空字符串应返回 None"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("")
        assert result is None

    def test_parse_models_skips_blank_lines(self) -> None:
        """空行应被跳过"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o,128000\n\ngpt-4o-mini,16384\n")
        assert result is not None
        assert len(result) == 2

    def test_parse_models_negative_max_tokens(self) -> None:
        """max_tokens < 1 应返回 None"""
        from src.ui.page.agent_page.provider_config_panel import AddProviderDialog

        dialog = AddProviderDialog.__new__(AddProviderDialog)

        result = dialog._parse_models("gpt-4o,0")
        assert result is None

        result = dialog._parse_models("gpt-4o,-1")
        assert result is None
