# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tool.py.

验证 ToolResult, ToolDefinition, ToolProvider 和 ToolRegistry 的核心行为. 
"""

from __future__ import annotations

# 标准库导入
import asyncio

# 第三方库导入
import pytest
from pydantic import BaseModel, Field

# 项目内模块导入
from src.core.agent.errors import DuplicateToolError, ValidationError
from src.core.agent.tool import (
    ToolDefinition,
    ToolProvider,
    ToolRegistry,
    ToolResult,
)


# ============================================================
# 测试辅助类
# ============================================================


class _EchoParams(BaseModel):
    """测试用参数模型."""

    message: str
    count: int = Field(default=1, ge=1)


class _EchoTool(ToolDefinition):
    """测试用工具: 回显消息."""

    tool_id = "echo"
    description = "Echo the message"
    parameters_schema = _EchoParams

    async def execute(self, params: BaseModel) -> ToolResult:
        p: _EchoParams = params  # type: ignore[assignment]
        return ToolResult(output=p.message * p.count)


class _FailingTool(ToolDefinition):
    """测试用工具: 总是抛出异常."""

    tool_id = "failing_tool"
    description = "Always fails"
    parameters_schema = _EchoParams

    async def execute(self, params: BaseModel) -> ToolResult:
        raise RuntimeError("Something went wrong")


class _DictSchemaTool(ToolDefinition):
    """测试用工具: 使用 JSON Schema dict 作为参数 schema."""

    tool_id = "dict_schema_tool"
    description = "Tool with dict schema"
    parameters_schema = {
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "age": {"type": "integer"},
        },
        "required": ["name"],
    }

    async def execute(self, params: BaseModel) -> ToolResult:
        return ToolResult(output=f"Hello {params.name}")  # type: ignore[attr-defined]


class _SimpleProvider(ToolProvider):
    """测试用工具提供者."""

    def __init__(self, tools: list[ToolDefinition]) -> None:
        self._tools = tools

    def get_tools(self) -> list[ToolDefinition]:
        return self._tools


def _make_tool(tool_id: str = "echo", description: str = "Echo tool") -> _EchoTool:
    """创建测试用工具实例."""
    tool = _EchoTool()
    tool.tool_id = tool_id
    tool.description = description
    return tool


# ============================================================
# ToolResult 测试
# ============================================================


class TestToolResult:
    """ToolResult 模型测试."""

    def test_basic_result(self) -> None:
        result = ToolResult(output="hello")
        assert result.output == "hello"
        assert result.is_error is False
        assert result.metadata is None

    def test_error_result(self) -> None:
        result = ToolResult(output="error msg", is_error=True)
        assert result.is_error is True

    def test_with_metadata(self) -> None:
        result = ToolResult(output="ok", metadata={"key": "value"})
        assert result.metadata == {"key": "value"}

    def test_empty_output(self) -> None:
        result = ToolResult(output="")
        assert result.output == ""


# ============================================================
# ToolRegistry.register() 测试
# ============================================================


class TestToolRegistryRegister:
    """ToolRegistry.register() 测试."""

    def test_register_valid_tool(self) -> None:
        registry = ToolRegistry()
        tool = _make_tool()
        registry.register(tool)
        assert registry.get("echo") is tool

    def test_register_duplicate_raises(self) -> None:
        registry = ToolRegistry()
        tool = _make_tool()
        registry.register(tool)
        with pytest.raises(DuplicateToolError) as exc_info:
            registry.register(tool)
        assert exc_info.value.tool_id == "echo"

    @pytest.mark.parametrize(
        "invalid_id",
        [
            "",  # 空字符串
            "A",  # 大写字母开头
            "1abc",  # 数字开头
            "_abc",  # 下划线开头
            "abc-def",  # 包含连字符
            "abc.def",  # 包含点号
            "abc def",  # 包含空格
            "a" * 65,  # 超过 64 字符
        ],
    )
    def test_register_invalid_tool_id_raises(self, invalid_id: str) -> None:
        registry = ToolRegistry()
        tool = _make_tool(tool_id=invalid_id)
        with pytest.raises(ValidationError) as exc_info:
            registry.register(tool)
        assert exc_info.value.field == "tool_id"

    @pytest.mark.parametrize(
        "valid_id",
        [
            "a",  # 单字符
            "file_read",  # 典型工具名
            "a" * 64,  # 恰好 64 字符
            "a1b2c3",  # 字母数字混合
            "tool_with_underscores",  # 下划线分隔
        ],
    )
    def test_register_valid_tool_id_accepted(self, valid_id: str) -> None:
        registry = ToolRegistry()
        tool = _make_tool(tool_id=valid_id)
        registry.register(tool)
        assert registry.get(valid_id).tool_id == valid_id


# ============================================================
# ToolRegistry.register_provider() 测试
# ============================================================


class TestToolRegistryProvider:
    """ToolRegistry.register_provider() 测试."""

    def test_register_provider_adds_tools(self) -> None:
        registry = ToolRegistry()
        tool_a = _make_tool(tool_id="tool_a", description="Tool A")
        tool_b = _make_tool(tool_id="tool_b", description="Tool B")
        provider = _SimpleProvider([tool_a, tool_b])
        registry.register_provider(provider)
        assert "tool_a" in registry.list_all()
        assert "tool_b" in registry.list_all()

    def test_register_provider_duplicate_raises(self) -> None:
        registry = ToolRegistry()
        tool = _make_tool(tool_id="dup_tool")
        registry.register(tool)
        provider = _SimpleProvider([_make_tool(tool_id="dup_tool")])
        with pytest.raises(DuplicateToolError):
            registry.register_provider(provider)


# ============================================================
# ToolRegistry.get() 测试
# ============================================================


class TestToolRegistryGet:
    """ToolRegistry.get() 测试."""

    def test_get_existing(self) -> None:
        registry = ToolRegistry()
        tool = _make_tool()
        registry.register(tool)
        assert registry.get("echo") is tool

    def test_get_nonexistent_raises(self) -> None:
        registry = ToolRegistry()
        with pytest.raises(KeyError):
            registry.get("nonexistent")


# ============================================================
# ToolRegistry.list_all() 测试
# ============================================================


class TestToolRegistryListAll:
    """ToolRegistry.list_all() 测试."""

    def test_list_all_empty(self) -> None:
        registry = ToolRegistry()
        assert registry.list_all() == {}

    def test_list_all_returns_id_description_map(self) -> None:
        registry = ToolRegistry()
        tool_a = _make_tool(tool_id="tool_a", description="Desc A")
        tool_b = _make_tool(tool_id="tool_b", description="Desc B")
        registry.register(tool_a)
        registry.register(tool_b)
        result = registry.list_all()
        assert result == {"tool_a": "Desc A", "tool_b": "Desc B"}


# ============================================================
# ToolRegistry.invoke() 测试
# ============================================================


class TestToolRegistryInvoke:
    """ToolRegistry.invoke() 测试."""

    def test_invoke_success(self) -> None:
        registry = ToolRegistry()
        registry.register(_EchoTool())
        result = asyncio.run(registry.invoke("echo", {"message": "hi", "count": 2}))
        assert result.output == "hihi"
        assert result.is_error is False

    def test_invoke_nonexistent_tool_raises(self) -> None:
        registry = ToolRegistry()
        with pytest.raises(KeyError):
            asyncio.run(registry.invoke("nonexistent", {}))

    def test_invoke_invalid_params_returns_error(self) -> None:
        registry = ToolRegistry()
        registry.register(_EchoTool())
        # message 是必填字段, 缺失应返回错误
        result = asyncio.run(registry.invoke("echo", {}))
        assert result.is_error is True
        assert "message" in result.output

    def test_invoke_param_constraint_violation(self) -> None:
        registry = ToolRegistry()
        registry.register(_EchoTool())
        # count 必须 >= 1
        result = asyncio.run(registry.invoke("echo", {"message": "hi", "count": 0}))
        assert result.is_error is True
        assert "count" in result.output

    def test_invoke_exception_wrapped_as_error(self) -> None:
        registry = ToolRegistry()
        registry.register(_FailingTool())
        result = asyncio.run(registry.invoke("failing_tool", {"message": "test"}))
        assert result.is_error is True
        assert "RuntimeError" in result.output
        assert "failing_tool" in result.output
        # 不应包含堆栈跟踪
        assert "Traceback" not in result.output

    def test_invoke_with_dict_schema_success(self) -> None:
        registry = ToolRegistry()
        registry.register(_DictSchemaTool())
        result = asyncio.run(registry.invoke("dict_schema_tool", {"name": "World", "age": 25}))
        assert result.is_error is False

    def test_invoke_with_dict_schema_missing_required(self) -> None:
        registry = ToolRegistry()
        registry.register(_DictSchemaTool())
        result = asyncio.run(registry.invoke("dict_schema_tool", {"age": 25}))
        assert result.is_error is True
        assert "name" in result.output

    def test_invoke_with_dict_schema_wrong_type(self) -> None:
        registry = ToolRegistry()
        registry.register(_DictSchemaTool())
        result = asyncio.run(registry.invoke("dict_schema_tool", {"name": "World", "age": "not_int"}))
        assert result.is_error is True
        assert "age" in result.output
