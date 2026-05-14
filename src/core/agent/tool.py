# -*- coding: utf-8 -*-
"""Tool 注册表与数据模型.

定义工具执行结果（ToolResult）、工具定义抽象基类（ToolDefinition）、
工具提供者接口（ToolProvider）以及工具注册表（ToolRegistry），
负责工具的注册、查询、参数验证与执行。
"""

from __future__ import annotations

import re
from abc import ABC, abstractmethod
from typing import Any

from pydantic import BaseModel, ValidationError as PydanticValidationError

from src.core.agent.errors import DuplicateToolError, ValidationError

# tool_id 格式正则：以小写字母开头，后跟 0-63 个小写字母/数字/下划线
_TOOL_ID_PATTERN = re.compile(r"^[a-z][a-z0-9_]{0,63}$")


class ToolResult(BaseModel):
    """工具执行结果.

    Attributes:
        output: 工具输出内容.
        is_error: 是否为错误结果.
        metadata: 可选的附加元数据.
    """

    output: str
    is_error: bool = False
    metadata: dict[str, Any] | None = None


class ToolDefinition(ABC):
    """工具定义抽象基类.

    所有工具必须继承此类并实现 execute 方法。

    Attributes:
        tool_id: 工具唯一标识，匹配 [a-z][a-z0-9_]{0,63}.
        description: 工具描述.
        parameters_schema: 参数 schema，可以是 pydantic model 类或 JSON Schema dict.
    """

    tool_id: str
    description: str
    parameters_schema: type[BaseModel] | dict

    @abstractmethod
    async def execute(self, params: BaseModel) -> ToolResult:
        """执行工具逻辑.

        Args:
            params: 经过验证的参数实例.

        Returns:
            工具执行结果.
        """
        ...


class ToolProvider(ABC):
    """工具提供者接口 - MCP 扩展点.

    实现此接口以批量提供工具定义，支持未来 MCP 集成。
    """

    @abstractmethod
    def get_tools(self) -> list[ToolDefinition]:
        """返回此 Provider 提供的所有工具定义.

        Returns:
            工具定义列表.
        """
        ...


class ToolRegistry:
    """工具注册表，管理工具的注册、查询与执行.

    支持单个工具注册、批量 Provider 注册、参数验证和安全执行。
    """

    def __init__(self) -> None:
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        """注册一个工具.

        Args:
            tool: 要注册的工具定义实例.

        Raises:
            ValidationError: 如果 tool_id 格式不合法.
            DuplicateToolError: 如果 tool_id 已存在.
        """
        if not _TOOL_ID_PATTERN.match(tool.tool_id):
            raise ValidationError(
                field="tool_id",
                reason=f"tool_id '{tool.tool_id}' does not match pattern [a-z][a-z0-9_]{{0,63}}",
            )
        if tool.tool_id in self._tools:
            raise DuplicateToolError(tool.tool_id)
        self._tools[tool.tool_id] = tool

    def register_provider(self, provider: ToolProvider) -> None:
        """注册一个工具提供者，将其所有工具加入注册表.

        Args:
            provider: 工具提供者实例.

        Raises:
            ValidationError: 如果任何工具的 tool_id 格式不合法.
            DuplicateToolError: 如果任何工具的 tool_id 已存在.
        """
        for tool in provider.get_tools():
            self.register(tool)

    def get(self, tool_id: str) -> ToolDefinition:
        """获取指定工具定义.

        Args:
            tool_id: 工具 ID.

        Returns:
            对应的工具定义实例.

        Raises:
            KeyError: 如果 tool_id 不存在.
        """
        if tool_id not in self._tools:
            raise KeyError(f"Tool '{tool_id}' not found in registry.")
        return self._tools[tool_id]

    def list_all(self) -> dict[str, str]:
        """列出所有已注册工具的 ID 和描述.

        Returns:
            字典，键为 tool_id，值为 description.
        """
        return {tid: tool.description for tid, tool in self._tools.items()}

    async def invoke(self, tool_id: str, params: dict) -> ToolResult:
        """调用指定工具.

        先验证参数，再执行工具逻辑。执行过程中的未处理异常会被包装为
        ToolResult(is_error=True)。

        Args:
            tool_id: 工具 ID.
            params: 参数字典.

        Returns:
            工具执行结果.

        Raises:
            KeyError: 如果 tool_id 不存在.
        """
        tool = self.get(tool_id)

        # 参数验证
        validated_params = self._validate_params(tool, params)
        if isinstance(validated_params, ToolResult):
            return validated_params

        # 执行工具，捕获未处理异常
        try:
            return await tool.execute(validated_params)
        except Exception as exc:
            return ToolResult(
                output=f"Tool '{tool_id}' raised {type(exc).__name__}: {exc}",
                is_error=True,
            )

    def _validate_params(
        self, tool: ToolDefinition, params: dict
    ) -> BaseModel | ToolResult:
        """验证参数.

        Args:
            tool: 工具定义.
            params: 原始参数字典.

        Returns:
            验证通过返回 BaseModel 实例，验证失败返回 ToolResult(is_error=True).
        """
        schema = tool.parameters_schema

        if isinstance(schema, type) and issubclass(schema, BaseModel):
            # pydantic model 验证
            try:
                return schema.model_validate(params)
            except PydanticValidationError as exc:
                # 提取第一个错误的字段名
                errors = exc.errors()
                if errors:
                    first_error = errors[0]
                    loc = first_error.get("loc", ())
                    field_name = ".".join(str(part) for part in loc) if loc else "unknown"
                    msg = first_error.get("msg", "validation failed")
                    return ToolResult(
                        output=f"Parameter validation failed for '{field_name}': {msg}",
                        is_error=True,
                    )
                return ToolResult(
                    output="Parameter validation failed",
                    is_error=True,
                )
        elif isinstance(schema, dict):
            # JSON Schema dict 验证 - 基础类型检查
            # 对于 dict schema，将 params 包装为简单的 BaseModel 实例
            # 这里做基本的 required 字段检查
            required_fields = schema.get("required", [])
            properties = schema.get("properties", {})

            for field_name in required_fields:
                if field_name not in params:
                    return ToolResult(
                        output=f"Parameter validation failed for '{field_name}': field required",
                        is_error=True,
                    )

            # 类型检查
            for field_name, value in params.items():
                if field_name in properties:
                    prop_schema = properties[field_name]
                    type_error = self._check_json_schema_type(
                        field_name, value, prop_schema
                    )
                    if type_error is not None:
                        return type_error

            # 创建一个动态 BaseModel 来传递参数
            dynamic_model = _create_dynamic_model(params)
            return dynamic_model
        else:
            return ToolResult(
                output="Invalid parameters_schema type on tool definition",
                is_error=True,
            )

    @staticmethod
    def _check_json_schema_type(
        field_name: str, value: Any, prop_schema: dict
    ) -> ToolResult | None:
        """检查值是否符合 JSON Schema 中定义的类型.

        Returns:
            验证失败返回 ToolResult，通过返回 None.
        """
        json_type = prop_schema.get("type")
        if json_type is None:
            return None

        type_map: dict[str, tuple[type, ...]] = {
            "string": (str,),
            "integer": (int,),
            "number": (int, float),
            "boolean": (bool,),
            "array": (list,),
            "object": (dict,),
        }

        expected_types = type_map.get(json_type)
        if expected_types is not None and not isinstance(value, expected_types):
            return ToolResult(
                output=f"Parameter validation failed for '{field_name}': expected type {json_type}",
                is_error=True,
            )
        return None


class _DynamicParams(BaseModel):
    """动态参数容器，用于 JSON Schema dict 验证后传递参数."""

    model_config = {"extra": "allow"}


def _create_dynamic_model(params: dict) -> BaseModel:
    """创建包含给定参数的动态 BaseModel 实例."""
    return _DynamicParams.model_validate(params)
