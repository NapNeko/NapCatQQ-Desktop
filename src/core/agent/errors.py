# -*- coding: utf-8 -*-
"""Agent 模块统一异常类型.

定义 Agent 框架中所有异常的层次结构，每个异常携带结构化的上下文信息，
便于上层代码进行精确的错误处理和用户友好的错误提示。
"""

from __future__ import annotations

from uuid import UUID


class AgentError(Exception):
    """Agent 模块所有异常的基类."""

    pass


class ValidationError(AgentError):
    """数据验证失败.

    Attributes:
        field: 验证失败的字段名.
        reason: 失败原因描述.
    """

    def __init__(self, field: str, reason: str) -> None:
        self.field = field
        self.reason = reason
        super().__init__(f"Validation failed on field '{field}': {reason}")


class NoActiveProviderError(AgentError):
    """未设置活跃 Provider.

    当查询活跃 Provider 但尚未设置时抛出。
    """

    def __init__(self) -> None:
        super().__init__("No active provider has been set. Please select a provider before use.")


class DuplicateProviderError(AgentError):
    """Provider ID 重复.

    Attributes:
        provider_id: 冲突的 provider_id.
    """

    def __init__(self, provider_id: str) -> None:
        self.provider_id = provider_id
        super().__init__(f"Provider with id '{provider_id}' already exists in the registry.")


class ModelNotFoundError(AgentError):
    """模型 ID 在 Provider 中不存在.

    Attributes:
        model_id: 未找到的模型 ID.
        provider_id: 所属 Provider 的 ID.
    """

    def __init__(self, model_id: str, provider_id: str) -> None:
        self.model_id = model_id
        self.provider_id = provider_id
        super().__init__(
            f"Model '{model_id}' not found in provider '{provider_id}'."
        )


class DuplicateToolError(AgentError):
    """Tool ID 重复.

    Attributes:
        tool_id: 冲突的 tool_id.
    """

    def __init__(self, tool_id: str) -> None:
        self.tool_id = tool_id
        super().__init__(f"Tool with id '{tool_id}' already exists in the registry.")


class SessionNotFoundError(AgentError):
    """Session ID 不存在.

    Attributes:
        session_id: 未找到的 session UUID.
    """

    def __init__(self, session_id: UUID) -> None:
        self.session_id = session_id
        super().__init__(f"Session '{session_id}' not found.")


class SessionCorruptedError(AgentError):
    """Session JSON 文件损坏.

    Attributes:
        session_id: 损坏的 session UUID.
    """

    def __init__(self, session_id: UUID) -> None:
        self.session_id = session_id
        super().__init__(
            f"Session '{session_id}' is corrupted and cannot be loaded."
        )


class StreamError(AgentError):
    """LLM 流式调用失败.

    Attributes:
        status_code: HTTP 状态码，连接超时等场景可能为 None.
        message: 错误描述信息.
    """

    def __init__(self, status_code: int | None, message: str) -> None:
        self.status_code = status_code
        self.message = message
        status_info = f"HTTP {status_code}" if status_code is not None else "No status"
        super().__init__(f"Stream error ({status_info}): {message}")


class PermissionDeniedError(AgentError):
    """权限被拒绝.

    Attributes:
        tool_id: 被拒绝的工具 ID.
        reason: 拒绝原因.
    """

    def __init__(self, tool_id: str, reason: str) -> None:
        self.tool_id = tool_id
        self.reason = reason
        super().__init__(f"Permission denied for tool '{tool_id}': {reason}")
