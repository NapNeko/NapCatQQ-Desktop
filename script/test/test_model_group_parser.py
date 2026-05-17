# -*- coding: utf-8 -*-
"""单元测试: parse_group_name 模型 ID 分组解析.

验证 parse_group_name 纯函数按连字符分割规则正确解析 group_name.

测试文件: script/test/test_model_group_parser.py
框架: pytest
"""

from __future__ import annotations

import pytest

from src.core.agent.model_group_parser import parse_group_name


class TestParseGroupName:
    """parse_group_name 单元测试."""

    # --- 基本分割规则 (Requirement 7.1) ---

    def test_stops_at_pure_digit_segment(self) -> None:
        """纯数字段触发停止: claude-3-opus → 'claude'."""
        assert parse_group_name("claude-3-opus-20240229") == "claude"

    def test_stops_at_segment_starting_with_digit(self) -> None:
        """以数字开头的段触发停止: gpt-4o → 'gpt'."""
        assert parse_group_name("gpt-4o-2024-05-13") == "gpt"

    def test_keeps_all_valid_prefix_segments(self) -> None:
        """所有前缀段均有效时全部保留: deepseek-r1 → 'deepseek-r1'."""
        assert parse_group_name("deepseek-r1") == "deepseek-r1"

    def test_multiple_valid_segments_before_digit(self) -> None:
        """多个有效段后遇到数字段: text-embedding-3-small → 'text-embedding'."""
        assert parse_group_name("text-embedding-3-small") == "text-embedding"

    def test_segment_starting_with_letter_not_digit(self) -> None:
        """以字母开头的段保留: o1-preview → 'o1-preview'."""
        assert parse_group_name("o1-preview") == "o1-preview"

    # --- 无连字符 (Requirement 7.2) ---

    def test_no_hyphen_returns_full_model_id(self) -> None:
        """无连字符时返回完整 model_id."""
        assert parse_group_name("gpt4") == "gpt4"

    def test_single_word_model_id(self) -> None:
        """单词 model_id 返回自身."""
        assert parse_group_name("llama") == "llama"

    # --- 无有效前缀段 (Requirement 7.3) ---

    def test_all_digit_segments_returns_full_model_id(self) -> None:
        """所有段均为纯数字时返回完整 model_id."""
        assert parse_group_name("123-456-789") == "123-456-789"

    def test_all_segments_start_with_digit(self) -> None:
        """所有段均以数字开头时返回完整 model_id."""
        assert parse_group_name("1a-2b-3c") == "1a-2b-3c"

    def test_first_segment_is_pure_digit(self) -> None:
        """第一段为纯数字时返回完整 model_id."""
        assert parse_group_name("3-gpt-turbo") == "3-gpt-turbo"

    # --- 前缀不变式 (Requirement 7.5) ---

    def test_invariant_startswith(self) -> None:
        """不变式: model_id.startswith(group_name) 或 group_name == model_id."""
        test_cases = [
            "gpt-4o-2024-05-13",
            "claude-3-opus-20240229",
            "deepseek-r1",
            "text-embedding-3-small",
            "o1-preview",
            "gpt4",
            "123-456-789",
            "1a-2b-3c",
        ]
        for model_id in test_cases:
            group_name = parse_group_name(model_id)
            assert model_id.startswith(group_name) or group_name == model_id, (
                f"Invariant violated for {model_id!r}: group_name={group_name!r}"
            )

    # --- 额外边界情况 ---

    def test_single_valid_segment_with_hyphen(self) -> None:
        """单个有效段后跟数字段: abc-123 → 'abc'."""
        assert parse_group_name("abc-123") == "abc"

    def test_all_valid_segments(self) -> None:
        """所有段均有效: a-b-c → 'a-b-c'."""
        assert parse_group_name("a-b-c") == "a-b-c"

    def test_mixed_valid_and_digit_start(self) -> None:
        """混合有效段和以数字开头的段: meta-llama-3b-instruct → 'meta-llama'."""
        assert parse_group_name("meta-llama-3b-instruct") == "meta-llama"

    def test_single_character_model_id(self) -> None:
        """单字符 model_id."""
        assert parse_group_name("x") == "x"

    def test_model_id_with_trailing_hyphen(self) -> None:
        """尾部连字符产生空段, 空段触发停止."""
        # "abc-" splits to ["abc", ""], empty segment stops
        assert parse_group_name("abc-") == "abc"

    def test_model_id_with_leading_hyphen(self) -> None:
        """前导连字符产生空段, 空段触发停止, 无有效前缀返回完整 model_id."""
        # "-abc" splits to ["", "abc"], first segment is empty → no valid prefix
        assert parse_group_name("-abc") == "-abc"

    def test_consecutive_hyphens(self) -> None:
        """连续连字符产生空段."""
        # "abc--def" splits to ["abc", "", "def"], empty segment stops after "abc"
        assert parse_group_name("abc--def") == "abc"
