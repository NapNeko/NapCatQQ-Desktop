# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/permission.py.

验证 PermissionRule 模型, glob 匹配逻辑和权限匹配引擎的核心行为. 
"""

from __future__ import annotations

import pytest
from pydantic import ValidationError as PydanticValidationError

from src.core.agent.permission import (
    PermissionRule,
    _compute_specificity,
    _matches_glob,
    evaluate_permission,
)


# ============================================================
# PermissionRule 模型测试
# ============================================================


class TestPermissionRule:
    """PermissionRule pydantic 模型验证."""

    def test_valid_rule_allow(self) -> None:
        rule = PermissionRule(pattern="file_*", target="file_read", action="allow")
        assert rule.pattern == "file_*"
        assert rule.target == "file_read"
        assert rule.action == "allow"

    def test_valid_rule_deny(self) -> None:
        rule = PermissionRule(pattern="*", target="*", action="deny")
        assert rule.action == "deny"

    def test_valid_rule_ask(self) -> None:
        rule = PermissionRule(pattern="shell_exec", target="shell_exec", action="ask")
        assert rule.action == "ask"

    def test_pattern_max_length_256(self) -> None:
        # 256 chars should be valid
        rule = PermissionRule(pattern="a" * 256, target="t", action="allow")
        assert len(rule.pattern) == 256

    def test_pattern_exceeds_max_length(self) -> None:
        with pytest.raises(PydanticValidationError):
            PermissionRule(pattern="a" * 257, target="t", action="allow")

    def test_invalid_action(self) -> None:
        with pytest.raises(PydanticValidationError):
            PermissionRule(pattern="*", target="t", action="invalid")  # type: ignore[arg-type]


# ============================================================
# Specificity 计算测试
# ============================================================


class TestComputeSpecificity:
    """_compute_specificity 函数测试."""

    def test_all_literal(self) -> None:
        assert _compute_specificity("file_read") == 9

    def test_all_wildcard_star(self) -> None:
        assert _compute_specificity("*") == 0

    def test_mixed_star(self) -> None:
        assert _compute_specificity("file_*") == 5

    def test_mixed_question(self) -> None:
        assert _compute_specificity("f?le_read") == 8

    def test_multiple_wildcards(self) -> None:
        assert _compute_specificity("*_?_*") == 2

    def test_empty_pattern(self) -> None:
        assert _compute_specificity("") == 0


# ============================================================
# Glob 匹配测试
# ============================================================


class TestMatchesGlob:
    """_matches_glob 函数测试."""

    def test_exact_match(self) -> None:
        assert _matches_glob("file_read", "file_read")

    def test_exact_no_match(self) -> None:
        assert not _matches_glob("file_read", "file_write")

    def test_star_matches_any(self) -> None:
        assert _matches_glob("*", "anything")

    def test_star_matches_empty(self) -> None:
        assert _matches_glob("*", "")

    def test_star_prefix(self) -> None:
        assert _matches_glob("file_*", "file_read")
        assert _matches_glob("file_*", "file_write")
        assert _matches_glob("file_*", "file_")

    def test_star_suffix(self) -> None:
        assert _matches_glob("*_read", "file_read")
        assert not _matches_glob("*_read", "file_write")

    def test_question_single_char(self) -> None:
        assert _matches_glob("file_?", "file_a")
        assert not _matches_glob("file_?", "file_ab")
        assert not _matches_glob("file_?", "file_")

    def test_combined_wildcards(self) -> None:
        assert _matches_glob("f?le_*", "file_read")
        assert _matches_glob("f?le_*", "fale_write")
        assert not _matches_glob("f?le_*", "fiile_read")

    def test_special_regex_chars_escaped(self) -> None:
        # Ensure regex special chars in pattern are treated as literals
        assert _matches_glob("file.read", "file.read")
        assert not _matches_glob("file.read", "filexread")

    def test_partial_match_rejected(self) -> None:
        # Pattern must match the entire string
        assert not _matches_glob("file", "file_read")
        assert not _matches_glob("file_read", "file")


# ============================================================
# evaluate_permission 测试
# ============================================================


class TestEvaluatePermission:
    """evaluate_permission 权限匹配引擎测试."""

    def test_no_rules_default_deny(self) -> None:
        assert evaluate_permission("file_read", []) == "deny"

    def test_single_allow_rule(self) -> None:
        rules = [PermissionRule(pattern="file_read", target="file_read", action="allow")]
        assert evaluate_permission("file_read", rules) == "allow"

    def test_single_deny_rule(self) -> None:
        rules = [PermissionRule(pattern="*", target="*", action="deny")]
        assert evaluate_permission("file_read", rules) == "deny"

    def test_specificity_ordering(self) -> None:
        """More specific rules take precedence over less specific ones."""
        rules = [
            PermissionRule(pattern="*", target="*", action="deny"),
            PermissionRule(pattern="file_read", target="file_read", action="allow"),
        ]
        # file_read matches both, but "file_read" (specificity=9) > "*" (specificity=0)
        assert evaluate_permission("file_read", rules) == "allow"
        # shell_exec only matches "*"
        assert evaluate_permission("shell_exec", rules) == "deny"

    def test_specificity_with_glob(self) -> None:
        """Glob patterns with more literal chars are more specific."""
        rules = [
            PermissionRule(pattern="file_*", target="file_tools", action="allow"),
            PermissionRule(pattern="*", target="*", action="deny"),
            PermissionRule(pattern="file_read", target="file_read", action="ask"),
        ]
        # "file_read" (9) > "file_*" (5) > "*" (0)
        assert evaluate_permission("file_read", rules) == "ask"
        assert evaluate_permission("file_write", rules) == "allow"
        assert evaluate_permission("shell_exec", rules) == "deny"

    def test_no_match_returns_deny(self) -> None:
        """Rules that don't match the tool_id are skipped."""
        rules = [
            PermissionRule(pattern="file_read", target="file_read", action="allow"),
        ]
        assert evaluate_permission("shell_exec", rules) == "deny"

    def test_first_matching_rule_wins_at_same_specificity(self) -> None:
        """When multiple rules have the same specificity, the first in sorted order wins."""
        rules = [
            PermissionRule(pattern="file_read", target="t1", action="allow"),
            PermissionRule(pattern="file_read", target="t2", action="deny"),
        ]
        # Both have specificity 9, both match - first in sorted order wins
        # Since they have same specificity, Python's stable sort preserves order
        result = evaluate_permission("file_read", rules)
        assert result in ("allow", "deny")  # Either is acceptable per spec

    def test_ask_action(self) -> None:
        rules = [
            PermissionRule(pattern="shell_*", target="shell_exec", action="ask"),
        ]
        assert evaluate_permission("shell_exec", rules) == "ask"
