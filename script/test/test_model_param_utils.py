# -*- coding: utf-8 -*-
"""单元测试: 模型参数钳制与消息内容转换纯函数.

测试 clamp_temperature、clamp_top_p、clamp_max_tokens 和 flatten_array_content
的核心行为和边界情况。
"""

from __future__ import annotations

from src.core.agent.model_param_utils import (
    clamp_max_tokens,
    clamp_temperature,
    clamp_top_p,
    flatten_array_content,
)


# --- clamp_temperature ---


class TestClampTemperature:
    """clamp_temperature 单元测试."""

    def test_value_within_range(self) -> None:
        assert clamp_temperature(0.7) == 0.7
        assert clamp_temperature(1.0) == 1.0
        assert clamp_temperature(0.0) == 0.0
        assert clamp_temperature(2.0) == 2.0

    def test_value_below_minimum(self) -> None:
        assert clamp_temperature(-1.0) == 0.0
        assert clamp_temperature(-100.5) == 0.0

    def test_value_above_maximum(self) -> None:
        assert clamp_temperature(2.1) == 2.0
        assert clamp_temperature(999.0) == 2.0

    def test_boundary_values(self) -> None:
        assert clamp_temperature(0.0) == 0.0
        assert clamp_temperature(2.0) == 2.0


# --- clamp_top_p ---


class TestClampTopP:
    """clamp_top_p 单元测试."""

    def test_value_within_range(self) -> None:
        assert clamp_top_p(0.5) == 0.5
        assert clamp_top_p(0.0) == 0.0
        assert clamp_top_p(1.0) == 1.0

    def test_value_below_minimum(self) -> None:
        assert clamp_top_p(-0.1) == 0.0
        assert clamp_top_p(-50.0) == 0.0

    def test_value_above_maximum(self) -> None:
        assert clamp_top_p(1.1) == 1.0
        assert clamp_top_p(100.0) == 1.0

    def test_boundary_values(self) -> None:
        assert clamp_top_p(0.0) == 0.0
        assert clamp_top_p(1.0) == 1.0


# --- clamp_max_tokens ---


class TestClampMaxTokens:
    """clamp_max_tokens 单元测试."""

    def test_value_within_range(self) -> None:
        assert clamp_max_tokens(100, 4096) == 100
        assert clamp_max_tokens(1, 4096) == 1
        assert clamp_max_tokens(4096, 4096) == 4096

    def test_value_below_minimum(self) -> None:
        assert clamp_max_tokens(0, 4096) == 1
        assert clamp_max_tokens(-10, 4096) == 1

    def test_value_above_maximum(self) -> None:
        assert clamp_max_tokens(5000, 4096) == 4096
        assert clamp_max_tokens(999999, 100) == 100

    def test_boundary_values(self) -> None:
        assert clamp_max_tokens(1, 1) == 1


# --- flatten_array_content ---


class TestFlattenArrayContent:
    """flatten_array_content 单元测试."""

    def test_single_text_block(self) -> None:
        content = [{"type": "text", "text": "Hello"}]
        assert flatten_array_content(content) == "Hello"

    def test_multiple_text_blocks(self) -> None:
        content = [
            {"type": "text", "text": "Hello"},
            {"type": "text", "text": "World"},
        ]
        assert flatten_array_content(content) == "Hello\nWorld"

    def test_mixed_content_discards_non_text(self) -> None:
        content = [
            {"type": "text", "text": "First"},
            {"type": "image_url", "image_url": {"url": "https://example.com/img.png"}},
            {"type": "text", "text": "Second"},
        ]
        assert flatten_array_content(content) == "First\nSecond"

    def test_only_image_url_blocks(self) -> None:
        content = [
            {"type": "image_url", "image_url": {"url": "https://example.com/a.png"}},
            {"type": "image_url", "image_url": {"url": "https://example.com/b.png"}},
        ]
        assert flatten_array_content(content) == ""

    def test_empty_content_list(self) -> None:
        assert flatten_array_content([]) == ""

    def test_text_block_with_empty_text(self) -> None:
        content = [{"type": "text", "text": ""}]
        assert flatten_array_content(content) == ""

    def test_preserves_original_order(self) -> None:
        content = [
            {"type": "text", "text": "A"},
            {"type": "image_url", "image_url": {"url": "..."}},
            {"type": "text", "text": "B"},
            {"type": "other", "data": "ignored"},
            {"type": "text", "text": "C"},
        ]
        assert flatten_array_content(content) == "A\nB\nC"

    def test_unknown_type_discarded(self) -> None:
        content = [
            {"type": "audio", "audio": "data"},
            {"type": "text", "text": "Only this"},
        ]
        assert flatten_array_content(content) == "Only this"
