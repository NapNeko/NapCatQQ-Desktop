# -*- coding: utf-8 -*-
"""Property-based tests for AgentEngine module.

Property 15: Agent tool filtering by permission.
Validates: Requirements 6.6

For any AgentDefinition with a set of PermissionRules, selecting that Agent for a
Session SHALL expose only the tools whose tool_ids are permitted (action="allow")
by the Agent's rules, and SHALL exclude all tools that are denied.
"""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import MagicMock

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from pydantic import BaseModel

from src.core.agent.agent_def import AgentDefinition
from src.core.agent.engine import AgentEngine
from src.core.agent.permission import PermissionRule, evaluate_permission
from src.core.agent.provider import ProviderRegistry
from src.core.agent.session import SessionManager
from src.core.agent.tool import ToolDefinition, ToolRegistry, ToolResult


# ============================================================
# Test Helpers
# ============================================================


class _DummyParams(BaseModel):
    """Minimal parameter model for test tools."""

    pass


class _DummyTool(ToolDefinition):
    """A concrete ToolDefinition for testing purposes."""

    def __init__(self, tool_id: str, description: str) -> None:
        self.tool_id = tool_id
        self.description = description
        self.parameters_schema = _DummyParams

    async def execute(self, params: BaseModel) -> ToolResult:
        return ToolResult(output="ok")


# ============================================================
# Strategies
# ============================================================

# Valid tool_id: starts with [a-z], followed by 1-15 chars of [a-z0-9_]
_valid_tool_id = st.from_regex(r"[a-z][a-z0-9_]{1,15}", fullmatch=True)

# Actions for permission rules
_action_strategy = st.sampled_from(["allow", "deny"])


@st.composite
def _tools_and_agent_with_permissions(
    draw: st.DrawFn,
) -> tuple[list[str], AgentDefinition]:
    """Generate a set of registered tool_ids and an AgentDefinition with permission rules.

    Returns a tuple of:
    - List of all tool_ids to register in the ToolRegistry
    - An AgentDefinition with permission_rules that allow some and deny others
    """
    # Generate 2-10 distinct tool_ids
    tool_ids = draw(
        st.lists(
            _valid_tool_id,
            min_size=2,
            max_size=10,
            unique=True,
        )
    )

    # Partition tool_ids into allowed and denied sets
    # Ensure at least one in each category when possible
    num_tools = len(tool_ids)
    if num_tools >= 2:
        # Pick a split point ensuring at least 1 in each group
        split = draw(st.integers(min_value=1, max_value=num_tools - 1))
        allowed_ids = tool_ids[:split]
        denied_ids = tool_ids[split:]
    else:
        # Edge case: only 1 tool, randomly allow or deny
        action = draw(_action_strategy)
        if action == "allow":
            allowed_ids = tool_ids
            denied_ids = []
        else:
            allowed_ids = []
            denied_ids = tool_ids

    # Build permission rules using exact match patterns (highest specificity)
    rules: list[PermissionRule] = []
    for tid in allowed_ids:
        rules.append(PermissionRule(pattern=tid, target=tid, action="allow"))
    for tid in denied_ids:
        rules.append(PermissionRule(pattern=tid, target=tid, action="deny"))

    # Create AgentDefinition with all tool_ids in tool_ids list
    # and the permission rules we built
    agent = AgentDefinition(
        name="test_agent",
        description="Test agent for property testing",
        mode="primary",
        system_prompt="",
        tool_ids=tool_ids,
        permission_rules=rules,
    )

    return tool_ids, agent


@st.composite
def _tools_and_agent_with_wildcard_deny(
    draw: st.DrawFn,
) -> tuple[list[str], AgentDefinition, set[str]]:
    """Generate tools with a wildcard deny rule and specific allow overrides.

    Returns:
    - List of all tool_ids to register
    - AgentDefinition with a wildcard deny + specific allows
    - Set of tool_ids that should be allowed (the specific overrides)
    """
    # Generate 3-8 distinct tool_ids
    tool_ids = draw(
        st.lists(
            _valid_tool_id,
            min_size=3,
            max_size=8,
            unique=True,
        )
    )

    # Pick 1 to N-1 tools to explicitly allow
    num_allowed = draw(st.integers(min_value=1, max_value=len(tool_ids) - 1))
    allowed_ids = set(tool_ids[:num_allowed])

    # Build rules: specific allows (higher specificity) + wildcard deny (lower specificity)
    rules: list[PermissionRule] = []
    for tid in allowed_ids:
        rules.append(PermissionRule(pattern=tid, target=tid, action="allow"))
    # Wildcard deny catches everything else
    rules.append(PermissionRule(pattern="*", target="*", action="deny"))

    agent = AgentDefinition(
        name="test_wildcard_agent",
        description="Test agent with wildcard deny",
        mode="primary",
        system_prompt="",
        tool_ids=tool_ids,
        permission_rules=rules,
    )

    return tool_ids, agent, allowed_ids


# ============================================================
# Property Tests
# ============================================================


class TestAgentToolFilteringByPermission:
    """Property 15: Agent tool filtering by permission.

    **Validates: Requirements 6.6**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(data=_tools_and_agent_with_permissions())
    def test_only_allowed_tools_exposed(
        self,
        data: tuple[list[str], AgentDefinition],
    ) -> None:
        """For any AgentDefinition with PermissionRules, _get_tool_definitions_for_agent()
        SHALL expose only the tools whose tool_ids are permitted (action="allow") and
        SHALL exclude all tools that are denied.

        **Validates: Requirements 6.6**
        """
        tool_ids, agent = data

        # Set up ToolRegistry with all tools registered
        tool_registry = ToolRegistry()
        for tid in tool_ids:
            tool_registry.register(_DummyTool(tool_id=tid, description=f"Tool {tid}"))

        # Create AgentEngine with minimal dependencies
        provider_registry = ProviderRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            session_manager = SessionManager(storage_dir=Path(tmpdir))
            engine = AgentEngine(
                provider_registry=provider_registry,
                tool_registry=tool_registry,
                session_manager=session_manager,
            )

            # Get tool definitions for the agent
            result = engine._get_tool_definitions_for_agent(agent)

            # Extract returned tool_ids
            returned_tool_ids = {
                td["function"]["name"] for td in result
            }

            # Compute expected: tools that are allowed by permission rules
            expected_allowed = set()
            for tid in tool_ids:
                permission = evaluate_permission(tid, agent.permission_rules)
                if permission != "deny":
                    expected_allowed.add(tid)

            assert returned_tool_ids == expected_allowed, (
                f"Expected tools {expected_allowed}, got {returned_tool_ids}. "
                f"Agent tool_ids={agent.tool_ids}, "
                f"Rules: {[(r.pattern, r.action) for r in agent.permission_rules]}"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(data=_tools_and_agent_with_wildcard_deny())
    def test_wildcard_deny_with_specific_allows(
        self,
        data: tuple[list[str], AgentDefinition, set[str]],
    ) -> None:
        """When a wildcard deny rule is present with specific allow overrides,
        only the specifically allowed tools SHALL be exposed (specificity ordering).

        **Validates: Requirements 6.6**
        """
        tool_ids, agent, expected_allowed = data

        # Set up ToolRegistry with all tools registered
        tool_registry = ToolRegistry()
        for tid in tool_ids:
            tool_registry.register(_DummyTool(tool_id=tid, description=f"Tool {tid}"))

        # Create AgentEngine with minimal dependencies
        provider_registry = ProviderRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            session_manager = SessionManager(storage_dir=Path(tmpdir))
            engine = AgentEngine(
                provider_registry=provider_registry,
                tool_registry=tool_registry,
                session_manager=session_manager,
            )

            # Get tool definitions for the agent
            result = engine._get_tool_definitions_for_agent(agent)

            # Extract returned tool_ids
            returned_tool_ids = {
                td["function"]["name"] for td in result
            }

            assert returned_tool_ids == expected_allowed, (
                f"Expected only {expected_allowed} to be allowed, got {returned_tool_ids}. "
                f"Rules: {[(r.pattern, r.action) for r in agent.permission_rules]}"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(data=_tools_and_agent_with_permissions())
    def test_denied_tools_never_in_result(
        self,
        data: tuple[list[str], AgentDefinition],
    ) -> None:
        """For any AgentDefinition, tools whose permission evaluates to "deny"
        SHALL never appear in the returned tool definitions.

        **Validates: Requirements 6.6**
        """
        tool_ids, agent = data

        # Set up ToolRegistry with all tools registered
        tool_registry = ToolRegistry()
        for tid in tool_ids:
            tool_registry.register(_DummyTool(tool_id=tid, description=f"Tool {tid}"))

        # Create AgentEngine with minimal dependencies
        provider_registry = ProviderRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            session_manager = SessionManager(storage_dir=Path(tmpdir))
            engine = AgentEngine(
                provider_registry=provider_registry,
                tool_registry=tool_registry,
                session_manager=session_manager,
            )

            # Get tool definitions for the agent
            result = engine._get_tool_definitions_for_agent(agent)

            # Extract returned tool_ids
            returned_tool_ids = {
                td["function"]["name"] for td in result
            }

            # Compute denied tools
            denied_tools = set()
            for tid in tool_ids:
                permission = evaluate_permission(tid, agent.permission_rules)
                if permission == "deny":
                    denied_tools.add(tid)

            # No denied tool should appear in the result
            intersection = returned_tool_ids & denied_tools
            assert intersection == set(), (
                f"Denied tools {intersection} should not appear in result. "
                f"Rules: {[(r.pattern, r.action) for r in agent.permission_rules]}"
            )
