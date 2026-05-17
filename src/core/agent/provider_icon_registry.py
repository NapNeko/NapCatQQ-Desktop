# -*- coding: utf-8 -*-
"""供应商图标注册表模块.

维护 provider_id → SVG 图标文件路径的映射关系，支持：
- 自定义绑定（用户手动设置的 provider_id → icon_filename）
- 直接匹配（{provider_id}-color.svg 文件存在）
- 别名匹配（通过 _PROVIDER_ALIASES 映射到 canonical icon_id）
- 未命中时返回 None，由调用方回退到首字母头像
"""

from __future__ import annotations

from pathlib import Path

from src.core.logging import LogSource, logger

from src.core.agent.config_persistence import ConfigPersistence

# 硬编码的 provider_id → canonical icon_id 别名映射
_PROVIDER_ALIASES: dict[str, str] = {
    "silicon": "siliconcloud",
    "siliconflow": "siliconcloud",
    "deepseek": "deepseek",
    "openai": "openai",
    "anthropic": "anthropic",
    "google": "google",
    "gemini": "google",
    "azure": "azure",
    "azure-openai": "azure",
    "mistral": "mistral",
    "cohere": "cohere",
    "groq": "groq",
    "perplexity": "perplexity",
    "together": "together-ai",
    "togetherai": "together-ai",
    "fireworks": "fireworks-ai",
    "moonshot": "moonshot",
    "kimi": "moonshot",
    "zhipu": "zhipu",
    "glm": "zhipu",
    "chatglm": "zhipu",
    "baichuan": "baichuan",
    "minimax": "minimax",
    "yi": "yi",
    "lingyiwanwu": "yi",
    "qwen": "qwen",
    "tongyi": "qwen",
    "dashscope": "qwen",
    "doubao": "doubao",
    "volcengine": "doubao",
    "spark": "spark",
    "xunfei": "spark",
    "ollama": "ollama",
    "huggingface": "huggingface",
    "replicate": "replicate",
    "cloudflare": "cloudflare",
    "novita": "novita",
    "openrouter": "openrouter",
}


class ProviderIconRegistry:
    """供应商图标注册表，维护 provider_id → SVG 文件路径映射。

    查找顺序：
    1. 自定义绑定 (custom_bindings)
    2. 直接匹配 {provider_id}-color.svg
    3. 别名匹配 _PROVIDER_ALIASES[provider_id] → {icon_id}-color.svg
    4. 返回 None（调用方回退到首字母头像）
    """

    def __init__(self, icons_dir: Path, config_persistence: ConfigPersistence) -> None:
        """初始化图标注册表。

        Args:
            icons_dir: SVG 图标文件存储目录路径。
            config_persistence: 配置持久化管理器，用于读写自定义绑定。
        """
        self._icons_dir = icons_dir
        self._config_persistence = config_persistence
        self._custom_bindings: dict[str, str] = {}

        # 从配置文件加载已有的自定义绑定
        self._load_custom_bindings()

    def resolve_icon_path(self, provider_id: str) -> Path | None:
        """查找 provider_id 对应的 SVG 图标路径。

        查找顺序：
        1. 自定义绑定 (custom_bindings)
        2. 直接匹配 {provider_id}-color.svg
        3. 别名匹配 _PROVIDER_ALIASES[provider_id] → {icon_id}-color.svg
        4. 返回 None（调用方回退到首字母头像）

        Args:
            provider_id: 供应商唯一标识符。

        Returns:
            SVG 图标文件的 Path，或 None（未找到匹配图标时）。
        """
        normalized_id = provider_id.lower().strip()

        # 1. 自定义绑定
        if normalized_id in self._custom_bindings:
            icon_filename = self._custom_bindings[normalized_id]
            icon_path = self._icons_dir / icon_filename
            if icon_path.is_file():
                return icon_path

        # 2. 直接匹配 {provider_id}-color.svg
        direct_path = self._icons_dir / f"{normalized_id}-color.svg"
        if direct_path.is_file():
            return direct_path

        # 3. 别名匹配
        canonical_id = _PROVIDER_ALIASES.get(normalized_id)
        if canonical_id is not None:
            alias_path = self._icons_dir / f"{canonical_id}-color.svg"
            if alias_path.is_file():
                return alias_path

        # 4. 未命中
        return None

    def set_custom_binding(self, provider_id: str, icon_filename: str) -> None:
        """设置自定义图标绑定并持久化。

        Args:
            provider_id: 供应商唯一标识符。
            icon_filename: 图标文件名（例如 "openai-color.svg"）。
        """
        normalized_id = provider_id.lower().strip()
        self._custom_bindings[normalized_id] = icon_filename

        # 持久化到配置文件
        config = self._config_persistence.load()
        config.custom_icon_bindings[normalized_id] = icon_filename
        self._config_persistence.save(config)

    def list_available_icons(self) -> list[str]:
        """列出所有可用的 SVG 图标文件名。

        Returns:
            icons_dir 目录下所有 .svg 文件的文件名列表，按字母排序。
        """
        if not self._icons_dir.is_dir():
            return []

        return sorted(
            f.name
            for f in self._icons_dir.iterdir()
            if f.is_file() and f.suffix.lower() == ".svg"
        )

    def _load_custom_bindings(self) -> None:
        """从配置文件加载自定义图标绑定。"""
        config = self._config_persistence.load()
        self._custom_bindings = dict(config.custom_icon_bindings)
