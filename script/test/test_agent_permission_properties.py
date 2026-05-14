# -*- coding: utf-8 -*-
"""Property-based tests for Permission module.

Property 14: Permission matching by specificity.
Validates: Requirements 7.2, 7.3

For any set of PermissionRules and any tool_id, the permission engine SHALL apply
the rule with the highest specificity (most non-wildcard literal characters in the
pattern) that matches the tool_id; if no rule matches, the invocation SHALL be denied.
"""

from __future__ import annotations

from typing import Literal

from hypothesis import HealthCheck, given, settings, assume
from hypothesis import strategies as st

from src.core.agent.permission import (
    PermissionRule,
    _compute_specificity,
    _matches_glob,
    evaluate_permission,
)


# ============================================================
# Strategies
# ============================================================

# Tool ID segments: lowercase letters and underscores (realistic tool IDs)
_tool_id_chars = st.sampled_from(
    list("abcdefghijklmnopqrstuvwxyz0123456789_")
)

# Generate realistic tool_ids (e.g., "file_read", "shell_exec")
_tool_id_strategy = st.from_regex(r"[a-z][a-z0-9_]{1,30}", fullmatch=True)

# Actions for permission rules
_action_strategy = st.sampled_from(["allow", "deny", "ask"])

# Generate glob patterns that are guaranteed to match a given tool_id
# by constructing patterns from the tool_id itself with optional wildcards


def _pattern_matching_tool_id(tool_id: str) -> st.SearchStrategy[str]:
    """Generate glob patterns guaranteed to match the given tool_id.

    Strategies:
    - Exact match (highest specificity)
    - Prefix + * (e.g., "file_*")
    - * + suffix (e.g., "*_read")
    - Single char replaced with ? (e.g., "f?le_read")
    - Pure wildcard "*" (lowest specificity)
    """
    patterns: list[str] = ["*"]  # Always include the catch-all

    if len(tool_id) >= 1:
        patterns.append(tool_id)  # Exact match

    if len(tool_id) >= 2:
        # Prefix patterns: take first N chars + *
        for i in range(1, len(tool_id)):
            patterns.append(tool_id[:i] + "*")

        # Suffix patterns: * + last N chars
        for i in range(1, len(tool_id)):
            patterns.append("*" + tool_id[i:])

        # Single ? replacement at each position
        for i in range(len(tool_id)):
            patterns.append(tool_id[:i] + "?" + tool_id[i + 1:])

    return st.sampled_from(patterns)


# Generate a set of PermissionRules with varying specificity that match a tool_id
@st.composite
def _rules_with_matching_tool_id(draw: st.DrawFn) -> tuple[str, list[PermissionRule]]:
    """Generate a tool_id and a list of PermissionRules where at least one matches."""
    tool_id = draw(_tool_id_strategy)

    # Generate 1-8 rules that match the tool_id (with varying specificity)
    num_matching = draw(st.integers(min_value=1, max_value=8))
    rules: list[PermissionRule] = []

    for _ in range(num_matching):
        pattern = draw(_pattern_matching_tool_id(tool_id))
        action = draw(_action_strategy)
        rules.append(PermissionRule(pattern=pattern, target=tool_id, action=action))

    # Optionally add some non-matching rules
    num_non_matching = draw(st.integers(min_value=0, max_value=4))
    for _ in range(num_non_matching):
        # Generate a pattern that won't match tool_id by using a different prefix
        non_match_prefix = draw(st.from_regex(r"zzz[a-z]{1,10}", fullmatch=True))
        action = draw(_action_strategy)
        rules.append(PermissionRule(pattern=non_match_prefix, target="other", action=action))

    return tool_id, rules


# Generate a tool_id and rules where NO rule matches
@st.composite
def _rules_not_matching_tool_id(draw: st.DrawFn) -> tuple[str, list[PermissionRule]]:
    """Generate a tool_id and a list of PermissionRules where none matches."""
    tool_id = draw(_tool_id_strategy)

    # Generate rules with patterns that definitely won't match tool_id
    num_rules = draw(st.integers(min_value=1, max_value=8))
    rules: list[PermissionRule] = []

    for _ in range(num_rules):
        # Use a completely different prefix that can't match tool_id
        non_match = draw(st.from_regex(r"zzz_nonmatch_[a-z]{1,10}", fullmatch=True))
        action = draw(_action_strategy)
        rules.append(PermissionRule(pattern=non_match, target="other", action=action))

    # Verify none of the rules actually match (defensive check)
    assume(not any(_matches_glob(r.pattern, tool_id) for r in rules))

    return tool_id, rules


# ============================================================
# Property Tests
# ============================================================


class TestPermissionMatchingBySpecificity:
    """Property 14: Permission matching by specificity.

    **Validates: Requirements 7.2, 7.3**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(data=_rules_with_matching_tool_id())
    def test_most_specific_matching_rule_applied(
        self,
        data: tuple[str, list[PermissionRule]],
    ) -> None:
        """For any set of PermissionRules and any tool_id, the permission engine
        SHALL apply the rule with the highest specificity (most non-wildcard literal
        characters in the pattern) that matches the tool_id.

        **Validates: Requirements 7.2, 7.3**
        """
        tool_id, rules = data

        # Compute expected result manually:
        # 1. Filter rules that match the tool_id
        # 2. Sort by specificity descending
        # 3. The first matching rule's action is the expected result
        matching_rules = [r for r in rules if _matches_glob(r.pattern, tool_id)]
        assert len(matching_rules) > 0, "At least one rule should match"

        # Sort by specificity descending (stable sort preserves order for ties)
        sorted_matching = sorted(
            matching_rules,
            key=lambda r: _compute_specificity(r.pattern),
            reverse=True,
        )
        expected_action = sorted_matching[0].action

        # Get actual result from evaluate_permission
        actual_action = evaluate_permission(tool_id, rules)

        # If there are ties in specificity at the top, any of the tied actions is valid
        max_specificity = _compute_specificity(sorted_matching[0].pattern)
        tied_actions = {
            r.action
            for r in sorted_matching
            if _compute_specificity(r.pattern) == max_specificity
        }

        assert actual_action in tied_actions, (
            f"For tool_id='{tool_id}', expected action in {tied_actions} "
            f"(max specificity={max_specificity}), got '{actual_action}'. "
            f"Rules: {[(r.pattern, r.action, _compute_specificity(r.pattern)) for r in rules]}"
        )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(data=_rules_not_matching_tool_id())
    def test_no_matching_rule_returns_deny(
        self,
        data: tuple[str, list[PermissionRule]],
    ) -> None:
        """If no Permission_Rule matches a tool invocation, the permission engine
        SHALL deny the invocation.

        **Validates: Requirements 7.2, 7.3**
        """
        tool_id, rules = data

        result = evaluate_permission(tool_id, rules)

        assert result == "deny", (
            f"Expected 'deny' when no rule matches tool_id='{tool_id}', "
            f"got '{result}'. "
            f"Rules: {[(r.pattern, r.action) for r in rules]}"
        )
