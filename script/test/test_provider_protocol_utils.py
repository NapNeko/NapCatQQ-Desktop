# -*- coding: utf-8 -*-
"""Unit tests for provider_protocol_utils module.

验证协议映射常量和纯函数的正确性，包括字段映射、路径映射、
徽章映射、标签映射以及 URL 预览生成。
"""

from __future__ import annotations

import pytest

from src.ui.page.setup_page.sub_page.provider_panel.provider_protocol_utils import (
    PROTOCOL_BADGE_MAP,
    PROTOCOL_FIELD_MAP,
    PROTOCOL_LABEL_MAP,
    PROTOCOL_PATH_MAP,
    build_url_preview,
    get_protocol_badge,
    get_protocol_fields,
    get_protocol_label,
    get_protocol_path,
)


# =============================================================================
# 常量验证
# =============================================================================


class TestProtocolConstants:
    """验证协议映射常量的完整性."""

    def test_protocol_field_map_has_all_known_types(self) -> None:
        """PROTOCOL_FIELD_MAP 包含所有已知协议类型."""
        assert set(PROTOCOL_FIELD_MAP.keys()) == {"openai", "anthropic", "gemini", "azure"}

    def test_protocol_field_map_openai(self) -> None:
        """OpenAI 字段集合正确."""
        assert PROTOCOL_FIELD_MAP["openai"] == {"api_key", "api_base_url"}

    def test_protocol_field_map_anthropic(self) -> None:
        """Anthropic 字段集合正确."""
        assert PROTOCOL_FIELD_MAP["anthropic"] == {"api_key", "api_base_url", "anthropic_version"}

    def test_protocol_field_map_gemini(self) -> None:
        """Gemini 字段集合正确."""
        assert PROTOCOL_FIELD_MAP["gemini"] == {"api_key", "api_base_url"}

    def test_protocol_field_map_azure(self) -> None:
        """Azure 字段集合正确."""
        assert PROTOCOL_FIELD_MAP["azure"] == {"api_key", "resource_endpoint", "deployment_name", "api_version"}

    def test_protocol_path_map_values(self) -> None:
        """PROTOCOL_PATH_MAP 路径映射正确."""
        assert PROTOCOL_PATH_MAP["openai"] == "/chat/completions"
        assert PROTOCOL_PATH_MAP["anthropic"] == "/messages"
        assert PROTOCOL_PATH_MAP["gemini"] == "/models"
        assert PROTOCOL_PATH_MAP["azure"] == "/chat/completions"

    def test_protocol_badge_map_values(self) -> None:
        """PROTOCOL_BADGE_MAP 徽章映射正确."""
        assert PROTOCOL_BADGE_MAP["openai"] == "OAI"
        assert PROTOCOL_BADGE_MAP["anthropic"] == "ANT"
        assert PROTOCOL_BADGE_MAP["gemini"] == "GEM"
        assert PROTOCOL_BADGE_MAP["azure"] == "AZR"

    def test_protocol_label_map_values(self) -> None:
        """PROTOCOL_LABEL_MAP 标签映射正确."""
        assert PROTOCOL_LABEL_MAP["openai"] == "OpenAI"
        assert PROTOCOL_LABEL_MAP["anthropic"] == "Anthropic"
        assert PROTOCOL_LABEL_MAP["gemini"] == "Gemini"
        assert PROTOCOL_LABEL_MAP["azure"] == "Azure"


# =============================================================================
# get_protocol_fields 测试
# =============================================================================


class TestGetProtocolFields:
    """验证 get_protocol_fields 纯函数."""

    def test_openai_fields(self) -> None:
        assert get_protocol_fields("openai") == {"api_key", "api_base_url"}

    def test_anthropic_fields(self) -> None:
        assert get_protocol_fields("anthropic") == {"api_key", "api_base_url", "anthropic_version"}

    def test_gemini_fields(self) -> None:
        assert get_protocol_fields("gemini") == {"api_key", "api_base_url"}

    def test_azure_fields(self) -> None:
        assert get_protocol_fields("azure") == {"api_key", "resource_endpoint", "deployment_name", "api_version"}

    def test_unknown_type_falls_back_to_openai(self) -> None:
        """未知协议类型回退到 openai 字段集."""
        assert get_protocol_fields("unknown") == {"api_key", "api_base_url"}
        assert get_protocol_fields("") == {"api_key", "api_base_url"}
        assert get_protocol_fields("custom_protocol") == {"api_key", "api_base_url"}


# =============================================================================
# get_protocol_path 测试
# =============================================================================


class TestGetProtocolPath:
    """验证 get_protocol_path 纯函数."""

    def test_openai_path(self) -> None:
        assert get_protocol_path("openai") == "/chat/completions"

    def test_anthropic_path(self) -> None:
        assert get_protocol_path("anthropic") == "/messages"

    def test_gemini_path(self) -> None:
        assert get_protocol_path("gemini") == "/models"

    def test_azure_path(self) -> None:
        assert get_protocol_path("azure") == "/chat/completions"

    def test_unknown_type_falls_back_to_chat_completions(self) -> None:
        """未知协议类型回退到 /chat/completions."""
        assert get_protocol_path("unknown") == "/chat/completions"
        assert get_protocol_path("") == "/chat/completions"


# =============================================================================
# get_protocol_label 测试
# =============================================================================


class TestGetProtocolLabel:
    """验证 get_protocol_label 纯函数."""

    def test_known_types(self) -> None:
        assert get_protocol_label("openai") == "OpenAI"
        assert get_protocol_label("anthropic") == "Anthropic"
        assert get_protocol_label("gemini") == "Gemini"
        assert get_protocol_label("azure") == "Azure"

    def test_unknown_type_capitalizes(self) -> None:
        """未知类型返回首字母大写形式."""
        assert get_protocol_label("custom") == "Custom"
        assert get_protocol_label("deepseek") == "Deepseek"

    def test_empty_string(self) -> None:
        """空字符串返回空字符串（capitalize 行为）."""
        assert get_protocol_label("") == ""


# =============================================================================
# get_protocol_badge 测试
# =============================================================================


class TestGetProtocolBadge:
    """验证 get_protocol_badge 纯函数."""

    def test_known_types(self) -> None:
        assert get_protocol_badge("openai") == "OAI"
        assert get_protocol_badge("anthropic") == "ANT"
        assert get_protocol_badge("gemini") == "GEM"
        assert get_protocol_badge("azure") == "AZR"

    def test_unknown_type_takes_first_3_chars_uppercased(self) -> None:
        """未知类型截取前 3 个字符并转为大写."""
        assert get_protocol_badge("custom") == "CUS"
        assert get_protocol_badge("deepseek") == "DEE"

    def test_short_unknown_type(self) -> None:
        """短于 3 字符的未知类型截取全部并转为大写."""
        assert get_protocol_badge("ab") == "AB"
        assert get_protocol_badge("x") == "X"

    def test_empty_string(self) -> None:
        """空字符串返回空字符串."""
        assert get_protocol_badge("") == ""


# =============================================================================
# build_url_preview 测试
# =============================================================================


class TestBuildUrlPreview:
    """验证 build_url_preview 纯函数."""

    def test_openai_basic(self) -> None:
        result = build_url_preview("https://api.openai.com/v1", "openai")
        assert result == "https://api.openai.com/v1/chat/completions"

    def test_anthropic_basic(self) -> None:
        result = build_url_preview("https://api.anthropic.com", "anthropic")
        assert result == "https://api.anthropic.com/messages"

    def test_gemini_basic(self) -> None:
        result = build_url_preview("https://generativelanguage.googleapis.com/v1beta", "gemini")
        assert result == "https://generativelanguage.googleapis.com/v1beta/models"

    def test_azure_with_api_version(self) -> None:
        result = build_url_preview(
            "https://myresource.openai.azure.com/openai/deployments/gpt-4",
            "azure",
            "2024-02-01",
        )
        assert result == (
            "https://myresource.openai.azure.com/openai/deployments/gpt-4"
            "/chat/completions?api-version=2024-02-01"
        )

    def test_azure_without_api_version(self) -> None:
        """Azure 协议但 azure_api_version 为空时不拼接 query param."""
        result = build_url_preview(
            "https://myresource.openai.azure.com",
            "azure",
            "",
        )
        assert result == "https://myresource.openai.azure.com/chat/completions"

    def test_trailing_slash_stripped(self) -> None:
        """api_base_url 末尾斜杠被去除."""
        result = build_url_preview("https://api.openai.com/v1/", "openai")
        assert result == "https://api.openai.com/v1/chat/completions"

    def test_multiple_trailing_slashes_stripped(self) -> None:
        """多个末尾斜杠被去除."""
        result = build_url_preview("https://api.openai.com/v1///", "openai")
        assert result == "https://api.openai.com/v1/chat/completions"

    def test_empty_url_returns_empty_string(self) -> None:
        """空 api_base_url 返回空字符串."""
        assert build_url_preview("", "openai") == ""

    def test_whitespace_only_url_returns_empty_string(self) -> None:
        """仅空白字符的 api_base_url 返回空字符串."""
        assert build_url_preview("   ", "openai") == ""
        assert build_url_preview("\t\n", "anthropic") == ""

    def test_unknown_protocol_uses_chat_completions(self) -> None:
        """未知协议类型使用 /chat/completions 路径."""
        result = build_url_preview("https://api.custom.com/v1", "unknown_protocol")
        assert result == "https://api.custom.com/v1/chat/completions"
