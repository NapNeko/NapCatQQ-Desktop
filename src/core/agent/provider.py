# -*- coding: utf-8 -*-
"""Provider 注册表与数据模型.

定义 LLM 提供商配置（Provider）、模型条目（ModelEntry）、模型配置（ModelConfig）
以及 ProviderRegistry 注册表，负责 Provider 的注册、查询、活跃状态管理。
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field, HttpUrl

from src.core.agent.errors import (
    DuplicateProviderError,
    ModelNotFoundError,
    NoActiveProviderError,
)


class ModelEntry(BaseModel):
    """Provider 支持的单个模型."""

    model_id: str = Field(min_length=1, max_length=128)
    display_name: str = ""
    max_tokens: int = Field(ge=1)
    supports_streaming: bool = True
    supports_tools: bool = True


class AzureConfig(BaseModel):
    """Azure-specific configuration."""

    resource_endpoint: str = Field(min_length=1)
    deployment_name: str = Field(min_length=1)
    api_version: str = Field(default="2024-02-01", min_length=1)


class Provider(BaseModel):
    """单个 LLM 提供商配置."""

    model_config = ConfigDict(extra="ignore")

    provider_id: str = Field(min_length=1, max_length=64)
    name: str = Field(min_length=1, max_length=128)
    api_base_url: HttpUrl
    api_key_ref: str = Field(min_length=1)
    models: list[ModelEntry] = Field(min_length=1)
    enabled: bool = True
    protocol_type: str = Field(default="openai")  # "openai" | "anthropic" | "gemini" | "azure"
    azure_config: AzureConfig | None = None
    website_url: str | None = None  # 官方网站 URL（用于 Header 外部链接）
    api_key_url: str | None = None  # API 密钥获取链接（用于 Help_Text 超链接）
    sort_order: int = 0  # 排序权重（拖拽排序持久化）


class ModelConfig(BaseModel):
    """模型配置对象，包含运行时参数."""

    model_id: str = Field(min_length=1, max_length=128)
    provider_id: str = Field(min_length=1, max_length=64)
    temperature: float = Field(default=0.7, ge=0.0, le=2.0)
    top_p: float = Field(default=1.0, ge=0.0, le=1.0)
    max_tokens: int = Field(ge=1)


class ProviderRegistry:
    """Provider 注册表，管理 LLM 提供商的注册与活跃状态.

    支持注册、注销、查询 Provider，以及设置/获取当前活跃的 Provider 和模型。
    """

    def __init__(self) -> None:
        self._providers: dict[str, Provider] = {}
        self._active_provider_id: str | None = None
        self._active_model_config: ModelConfig | None = None

    def register(self, provider: Provider) -> None:
        """注册一个 Provider.

        Args:
            provider: 要注册的 Provider 实例.

        Raises:
            DuplicateProviderError: 如果 provider_id 已存在.
        """
        if provider.provider_id in self._providers:
            raise DuplicateProviderError(provider.provider_id)
        self._providers[provider.provider_id] = provider

    def unregister(self, provider_id: str) -> None:
        """注销一个 Provider.

        Args:
            provider_id: 要注销的 Provider ID.

        Raises:
            KeyError: 如果 provider_id 不存在.
        """
        if provider_id not in self._providers:
            raise KeyError(f"Provider '{provider_id}' not found in registry.")
        del self._providers[provider_id]
        # 如果注销的是当前活跃 Provider，清除活跃状态
        if self._active_provider_id == provider_id:
            self._active_provider_id = None
            self._active_model_config = None

    def get(self, provider_id: str) -> Provider:
        """获取指定 Provider.

        Args:
            provider_id: Provider ID.

        Returns:
            对应的 Provider 实例.

        Raises:
            KeyError: 如果 provider_id 不存在.
        """
        if provider_id not in self._providers:
            raise KeyError(f"Provider '{provider_id}' not found in registry.")
        return self._providers[provider_id]

    def list_all(self) -> list[Provider]:
        """列出所有已注册的 Provider, 按 sort_order 升序排列.

        Returns:
            所有 Provider 实例的列表, 按 sort_order 排序.
        """
        return sorted(self._providers.values(), key=lambda p: p.sort_order)

    def list_enabled(self) -> list[Provider]:
        """列出所有启用的 Provider.

        Returns:
            enabled == True 的 Provider 实例列表.
        """
        return [p for p in self._providers.values() if p.enabled]

    def set_active(self, provider_id: str, model_id: str) -> None:
        """设置活跃的 Provider 和模型.

        Args:
            provider_id: Provider ID.
            model_id: 模型 ID，必须存在于该 Provider 的 models 列表中.

        Raises:
            KeyError: 如果 provider_id 不存在.
            ModelNotFoundError: 如果 model_id 不在 Provider 的 models 列表中.
        """
        if provider_id not in self._providers:
            raise KeyError(f"Provider '{provider_id}' not found in registry.")

        provider = self._providers[provider_id]

        # 验证 model_id 存在于 Provider 的 models 列表中
        model_entry: ModelEntry | None = None
        for entry in provider.models:
            if entry.model_id == model_id:
                model_entry = entry
                break

        if model_entry is None:
            raise ModelNotFoundError(model_id, provider_id)

        self._active_provider_id = provider_id
        self._active_model_config = ModelConfig(
            model_id=model_id,
            provider_id=provider_id,
            max_tokens=model_entry.max_tokens,
        )

    def set_enabled(self, provider_id: str, enabled: bool) -> None:
        """设置供应商启用状态.

        Args:
            provider_id: Provider ID.
            enabled: 是否启用.

        Raises:
            KeyError: 如果 provider_id 不存在.
        """
        if provider_id not in self._providers:
            raise KeyError(f"Provider '{provider_id}' not found in registry.")
        provider = self._providers[provider_id]
        self._providers[provider_id] = provider.model_copy(update={"enabled": enabled})
        # 如果禁用的是当前活跃供应商，清除活跃状态
        if not enabled and self._active_provider_id == provider_id:
            self._active_provider_id = None
            self._active_model_config = None

    def update_provider(self, provider_id: str, **kwargs) -> None:
        """更新供应商字段.

        使用 pydantic 的 model_copy(update=...) 创建新实例替换旧实例。
        支持更新 api_base_url、api_key_ref、name、models 等字段。

        Args:
            provider_id: 要更新的 Provider ID.
            **kwargs: 要更新的字段键值对.

        Raises:
            KeyError: 如果 provider_id 不存在.
        """
        if provider_id not in self._providers:
            raise KeyError(f"Provider '{provider_id}' not found in registry.")
        provider = self._providers[provider_id]
        self._providers[provider_id] = provider.model_copy(update=kwargs)

    def reorder_providers(self, ordered_ids: list[str]) -> None:
        """按给定顺序更新所有 Provider 的 sort_order 字段.

        遍历 ordered_ids 列表, 将每个 Provider 的 sort_order 设为其索引值 (0, 1, 2, ...).
        不在列表中的 Provider 保持原有 sort_order 不变.

        Args:
            ordered_ids: 按期望顺序排列的 provider_id 列表.
        """
        for index, provider_id in enumerate(ordered_ids):
            if provider_id in self._providers:
                provider = self._providers[provider_id]
                self._providers[provider_id] = provider.model_copy(
                    update={"sort_order": index}
                )

    def get_active(self) -> tuple[Provider, ModelConfig]:
        """获取当前活跃的 Provider 和 ModelConfig.

        Returns:
            (Provider, ModelConfig) 元组.

        Raises:
            NoActiveProviderError: 如果未设置活跃 Provider.
        """
        if self._active_provider_id is None or self._active_model_config is None:
            raise NoActiveProviderError()

        provider = self._providers[self._active_provider_id]
        return provider, self._active_model_config
