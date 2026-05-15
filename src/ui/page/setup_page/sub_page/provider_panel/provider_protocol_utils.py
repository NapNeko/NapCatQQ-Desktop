# -*- coding: utf-8 -*-
"""协议映射常量和纯函数模块.

提供协议类型 (OpenAI/Anthropic/Gemini/Azure) 的字段映射, 路径映射,
徽章映射, 标签映射以及 URL 预览生成等纯函数, 供 UI 组件调用.
"""

from __future__ import annotations

# =============================================================================
# 协议映射常量
# =============================================================================

PROTOCOL_FIELD_MAP: dict[str, set[str]] = {
    "openai": {"api_key", "api_base_url"},
    "anthropic": {"api_key", "api_base_url", "anthropic_version"},
    "gemini": {"api_key", "api_base_url"},
    "azure": {"api_key", "resource_endpoint", "deployment_name", "api_version"},
}

PROTOCOL_PATH_MAP: dict[str, str] = {
    "openai": "/chat/completions",
    "anthropic": "/messages",
    "gemini": "/models",
    "azure": "/chat/completions",  # api_version 作为 query param 拼接
}

PROTOCOL_BADGE_MAP: dict[str, str] = {
    "openai": "OAI",
    "anthropic": "ANT",
    "gemini": "GEM",
    "azure": "AZR",
}

PROTOCOL_LABEL_MAP: dict[str, str] = {
    "openai": "OpenAI",
    "anthropic": "Anthropic",
    "gemini": "Gemini",
    "azure": "Azure",
}


# =============================================================================
# 纯函数
# =============================================================================


def build_url_preview(
    api_base_url: str,
    protocol_type: str,
    azure_api_version: str = "",
) -> str:
    """构建 URL 预览字符串.

    Args:
        api_base_url: API 基础地址.
        protocol_type: 协议类型.
        azure_api_version: Azure API 版本 (仅 azure 协议使用).

    Returns:
        完整的请求 URL 预览字符串, 若 api_base_url 为空则返回空字符串.
    """
    if not api_base_url.strip():
        return ""

    base = api_base_url.rstrip("/")
    path = get_protocol_path(protocol_type)

    if protocol_type == "azure" and azure_api_version:
        return f"{base}{path}?api-version={azure_api_version}"

    return f"{base}{path}"


def get_protocol_label(protocol_type: str) -> str:
    """获取协议类型的显示标签.

    已知类型返回映射值 ("OpenAI", "Anthropic", "Gemini", "Azure"),
    未知类型返回原始字符串的首字母大写形式.

    Args:
        protocol_type: 协议类型字符串.

    Returns:
        协议类型的显示标签.
    """
    if protocol_type in PROTOCOL_LABEL_MAP:
        return PROTOCOL_LABEL_MAP[protocol_type]
    return protocol_type.capitalize()


def get_protocol_badge(protocol_type: str) -> str:
    """获取协议类型的徽章缩写.

    映射: "openai"→"OAI", "anthropic"→"ANT", "gemini"→"GEM", "azure"→"AZR".
    未知类型截取前 3 个字符并转为大写.

    Args:
        protocol_type: 协议类型字符串.

    Returns:
        协议类型的徽章缩写文本.
    """
    if protocol_type in PROTOCOL_BADGE_MAP:
        return PROTOCOL_BADGE_MAP[protocol_type]
    return protocol_type[:3].upper()


def get_protocol_fields(protocol_type: str) -> set[str]:
    """获取指定协议类型应显示的字段集合.

    未知协议类型回退到 openai 字段集.

    Args:
        protocol_type: 协议类型字符串.

    Returns:
        字段名称集合, 如 {"api_key", "api_base_url"} 或
        {"api_key", "resource_endpoint", "deployment_name", "api_version"}.
    """
    return PROTOCOL_FIELD_MAP.get(protocol_type, PROTOCOL_FIELD_MAP["openai"])


def get_protocol_path(protocol_type: str) -> str:
    """获取协议类型对应的 URL 路径后缀.

    "openai" → "/chat/completions"
    "anthropic" → "/messages"
    "gemini" → "/models"
    "azure" → "/chat/completions"
    未知 → "/chat/completions"

    Args:
        protocol_type: 协议类型字符串.

    Returns:
        URL 路径后缀字符串.
    """
    return PROTOCOL_PATH_MAP.get(protocol_type, "/chat/completions")
