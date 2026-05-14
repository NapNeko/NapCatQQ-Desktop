# -*- coding: utf-8 -*-
"""Property-based tests for Tool module.

Property 6: Tool registration and listing invariant.
Validates: Requirements 3.1, 3.6

Property 8: Tool parameter validation before execution.
Validates: Requirements 3.4, 3.5

Property 9: Tool exception wrapping.
Validates: Requirements 3.8
"""

from __future__ import annotations

# 第三方库导入
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from pydantic import BaseModel

# 项目内模块导入
from src.core.agent.tool import ToolDefinition, ToolRegistry, ToolResult


# --- Test helper ---


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


# --- Strategies ---

# Valid tool_id: starts with [a-z], followed by 0-63 chars of [a-z0-9_]
valid_tool_id = st.from_regex(r"[a-z][a-z0-9_]{0,63}", fullmatch=True)

# Valid description: at least 1 non-whitespace character
valid_description = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N", "P", "S")),
    min_size=1,
    max_size=128,
)

# Strategy for a set of distinct (tool_id, description) pairs
tool_definitions_strategy = st.lists(
    st.tuples(valid_tool_id, valid_description),
    min_size=0,
    max_size=20,
    unique_by=lambda t: t[0],  # ensure distinct tool_ids
)


# --- Property Test ---


class TestToolRegistrationAndListingInvariant:
    """Property 6: Tool registration and listing invariant.

    **Validates: Requirements 3.1, 3.6**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(tool_defs=tool_definitions_strategy)
    def test_register_n_tools_list_all_returns_n_matching_entries(
        self,
        tool_defs: list[tuple[str, str]],
    ) -> None:
        """For any set of N valid ToolDefinitions with distinct tool_ids,
        registering all and calling list_all() returns exactly N entries
        with matching tool_ids and descriptions.

        **Validates: Requirements 3.1, 3.6**
        """
        registry = ToolRegistry()

        # Register all tools
        for tool_id, description in tool_defs:
            tool = _DummyTool(tool_id=tool_id, description=description)
            registry.register(tool)

        # Call list_all()
        result = registry.list_all()

        # Verify exactly N entries
        assert len(result) == len(tool_defs), (
            f"Expected {len(tool_defs)} entries, got {len(result)}"
        )

        # Verify each entry's tool_id and description matches
        for tool_id, description in tool_defs:
            assert tool_id in result, (
                f"tool_id '{tool_id}' not found in list_all() result"
            )
            assert result[tool_id] == description, (
                f"Description mismatch for '{tool_id}': "
                f"expected '{description}', got '{result[tool_id]}'"
            )


# ============================================================
# Property 9: Tool exception wrapping
# ============================================================

import asyncio


def _make_raising_tool(tool_id: str, exc_type: type[Exception], exc_msg: str) -> ToolDefinition:
    """Create a ToolDefinition that raises the given exception type on execute."""

    class _RaisingTool(ToolDefinition):
        parameters_schema = _DummyParams
        description = f"Tool that raises {exc_type.__name__}"

        async def execute(self, params: BaseModel) -> ToolResult:
            raise exc_type(exc_msg)

    tool = _RaisingTool()
    tool.tool_id = tool_id
    return tool


# Exception types strategy
_exception_types = st.sampled_from([
    ValueError,
    RuntimeError,
    TypeError,
    KeyError,
    IndexError,
    AttributeError,
    IOError,
    OSError,
    ZeroDivisionError,
    NotImplementedError,
    PermissionError,
    FileNotFoundError,
    ConnectionError,
    TimeoutError,
])

# Exception messages: non-empty strings that don't contain "Traceback"
_exception_messages = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N", "Z", "P")),
    min_size=1,
    max_size=100,
).filter(lambda s: "Traceback" not in s)


class TestToolExceptionWrapping:
    """Property 9: Tool exception wrapping.

    **Validates: Requirements 3.8**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        tool_id=valid_tool_id,
        exc_type=_exception_types,
        exc_msg=_exception_messages,
    )
    def test_unhandled_exception_wrapped_as_error_result(
        self,
        tool_id: str,
        exc_type: type[Exception],
        exc_msg: str,
    ) -> None:
        """For any tool raising an unhandled exception, invoke() returns ToolResult
        with is_error=True, output containing exception type name and tool_id,
        and no stack trace exposure.

        **Validates: Requirements 3.8**
        """
        # Arrange: create a registry and register a tool that raises the given exception
        registry = ToolRegistry()
        tool = _make_raising_tool(tool_id, exc_type, exc_msg)
        registry.register(tool)

        # Act: invoke the tool
        result = asyncio.run(registry.invoke(tool_id, {}))

        # Assert 1: result is an error
        assert result.is_error is True, (
            f"Expected is_error=True for tool '{tool_id}' raising {exc_type.__name__}"
        )

        # Assert 2: output contains the exception type name
        assert exc_type.__name__ in result.output, (
            f"Expected '{exc_type.__name__}' in output, got: {result.output}"
        )

        # Assert 3: output contains the tool_id
        assert tool_id in result.output, (
            f"Expected tool_id '{tool_id}' in output, got: {result.output}"
        )

        # Assert 4: output does NOT contain "Traceback" (no stack trace exposure)
        assert "Traceback" not in result.output, (
            f"Output should not contain stack traces, got: {result.output}"
        )


# ============================================================
# Property 8: Tool parameter validation before execution
# ============================================================

from typing import Any

from pydantic import Field


class _StrictParams(BaseModel):
    """Pydantic parameters schema with required fields and type constraints."""

    name: str = Field(min_length=1, max_length=100)
    age: int = Field(ge=0, le=200)
    email: str = Field(min_length=3)


class _TrackedTool(ToolDefinition):
    """A tool that tracks whether execute() was called."""

    tool_id = "tracked_tool"
    description = "A tool that tracks execution calls"
    parameters_schema = _StrictParams

    def __init__(self) -> None:
        super().__init__()
        self.execute_count = 0

    async def execute(self, params: BaseModel) -> ToolResult:
        self.execute_count += 1
        return ToolResult(output="executed")


# --- Strategies for generating invalid parameters ---

# Strategy: missing required field "name"
_missing_name_params = st.fixed_dictionaries(
    {
        "age": st.integers(min_value=0, max_value=200),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: missing required field "age"
_missing_age_params = st.fixed_dictionaries(
    {
        "name": st.text(min_size=1, max_size=100),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: missing required field "email"
_missing_email_params = st.fixed_dictionaries(
    {
        "name": st.text(min_size=1, max_size=100),
        "age": st.integers(min_value=0, max_value=200),
    }
)

# Strategy: wrong type for "name" (not a string)
_wrong_type_name_params = st.fixed_dictionaries(
    {
        "name": st.one_of(
            st.integers(),
            st.lists(st.integers(), max_size=3),
            st.booleans(),
        ),
        "age": st.integers(min_value=0, max_value=200),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: wrong type for "age" (not coercible to integer)
# Note: pydantic coerces numeric strings to int, so we use types that cannot be coerced
_wrong_type_age_params = st.fixed_dictionaries(
    {
        "name": st.text(min_size=1, max_size=100),
        "age": st.one_of(
            st.lists(st.integers(), min_size=1, max_size=3),
            st.dictionaries(st.text(min_size=1, max_size=5), st.integers(), min_size=1, max_size=3),
        ),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: "name" violates min_length=1 (empty string)
_empty_name_params = st.fixed_dictionaries(
    {
        "name": st.just(""),
        "age": st.integers(min_value=0, max_value=200),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: "age" violates ge=0 (negative)
_negative_age_params = st.fixed_dictionaries(
    {
        "name": st.text(min_size=1, max_size=100),
        "age": st.integers(max_value=-1),
        "email": st.text(min_size=3, max_size=50),
    }
)

# Strategy: "age" violates le=200 (too large)
_large_age_params = st.fixed_dictionaries(
    {
        "name": st.text(min_size=1, max_size=100),
        "age": st.integers(min_value=201, max_value=10000),
        "email": st.text(min_size=3, max_size=50),
    }
)


class TestToolParameterValidationBeforeExecution:
    """Property 8: Tool parameter validation before execution.

    **Validates: Requirements 3.4, 3.5**

    For any registered tool and any parameters that fail validation against the
    tool's parameters_schema, invoking the tool SHALL return a ToolResult with
    is_error=True and a message containing the name of the invalid parameter,
    without calling the tool's execute method.
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_missing_name_params)
    def test_missing_required_field_name(self, params: dict[str, Any]) -> None:
        """Missing required field 'name' returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "name" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_missing_age_params)
    def test_missing_required_field_age(self, params: dict[str, Any]) -> None:
        """Missing required field 'age' returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "age" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_missing_email_params)
    def test_missing_required_field_email(self, params: dict[str, Any]) -> None:
        """Missing required field 'email' returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "email" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_wrong_type_name_params)
    def test_wrong_type_for_name(self, params: dict[str, Any]) -> None:
        """Wrong type for 'name' returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "name" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_wrong_type_age_params)
    def test_wrong_type_for_age(self, params: dict[str, Any]) -> None:
        """Wrong type for 'age' returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "age" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_empty_name_params)
    def test_constraint_violation_empty_name(self, params: dict[str, Any]) -> None:
        """Empty 'name' violates min_length constraint, returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "name" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_negative_age_params)
    def test_constraint_violation_negative_age(self, params: dict[str, Any]) -> None:
        """Negative 'age' violates ge=0 constraint, returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "age" in result.output
        assert tool.execute_count == 0

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(params=_large_age_params)
    def test_constraint_violation_age_too_large(self, params: dict[str, Any]) -> None:
        """Age > 200 violates le=200 constraint, returns error without executing.

        **Validates: Requirements 3.4, 3.5**
        """
        tool = _TrackedTool()
        registry = ToolRegistry()
        registry.register(tool)

        result = asyncio.run(registry.invoke("tracked_tool", params))

        assert result.is_error is True
        assert "age" in result.output
        assert tool.execute_count == 0
