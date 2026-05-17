# -*- coding: utf-8 -*-
"""Protocol Adapter 抽象层.

定义协议适配器接口 (ProtocolAdapter) , HTTP 请求规格 (HttpRequestSpec) , 
以及适配器注册表 (AdapterRegistry) , 支持多 LLM 提供商的原生协议通信. 

Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 2.1, 2.3, 2.5
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import TYPE_CHECKING, AsyncIterator

from src.core.logging import LogSource, logger

from src.core.agent.errors import ValidationError
from src.core.agent.stream import StreamEvent
from src.core.agent.tool import ToolResult

if TYPE_CHECKING:
    from src.core.agent.provider import ModelConfig, Provider
    from src.core.agent.session import Message


@dataclass(frozen=True)
class HttpRequestSpec:
    """HTTP 请求规格, 由 build_request 返回.

    Attributes:
        method: HTTP 方法 (LLM 调用始终为 "POST") .
        url: 完整请求 URL (含查询参数) .
        headers: HTTP 请求头字典.
        body: JSON 可序列化的请求体字典.
    """

    method: str
    url: str
    headers: dict[str, str]
    body: dict


class ProtocolAdapter(ABC):
    """协议适配器抽象基类.

    定义将内部消息/工具表示转换为提供商特定 HTTP 请求载荷, 
    以及解析提供商特定流式响应为 StreamEvent 序列的接口. 
    """

    @abstractmethod
    def build_request(
        self,
        messages: list[Message],
        tool_definitions: list[dict],
        model_config: ModelConfig,
        provider: Provider,
    ) -> HttpRequestSpec:
        """构建 LLM 调用的 HTTP 请求规格.

        Args:
            messages: 内部消息历史列表.
            tool_definitions: OpenAI 格式的工具定义列表.
            model_config: 模型配置 (model_id, temperature 等) .
            provider: Provider 实例 (用于 URL, 密钥等) .

        Returns:
            包含 method, url, headers, body 的 HttpRequestSpec.

        Raises:
            ValidationError: 如果 messages 列表为空.
        """
        ...

    @abstractmethod
    async def parse_stream(
        self, response_lines: AsyncIterator[str]
    ) -> AsyncIterator[StreamEvent]:
        """解析提供商特定的流式响应为 StreamEvent 序列.

        Args:
            response_lines: 原始响应行/块的异步迭代器.

        Yields:
            StreamEvent 实例 (TextDelta, ToolCallStart 等) .
            遇到格式错误数据时, yield StreamErrorEvent 并停止.
        """
        ...
        # 使 async generator 类型检查通过
        yield  # type: ignore[misc]  # pragma: no cover

    @abstractmethod
    def build_headers(self, api_key: str) -> dict[str, str]:
        """构建提供商特定的认证请求头.

        Args:
            api_key: API 密钥字符串.

        Returns:
            HTTP 请求头字典.
        """
        ...

    @abstractmethod
    def build_tool_result_payload(
        self, tool_call_id: str, tool_result: ToolResult
    ) -> list[dict]:
        """构建提供商特定的工具结果消息载荷.

        Args:
            tool_call_id: 要响应的工具调用 ID.
            tool_result: 工具执行结果.

        Returns:
            提供商特定格式的消息字典列表.
        """
        ...


def _validate_messages_not_empty(messages: list[Message]) -> None:
    """验证消息列表非空, 供子类在 build_request 中调用.

    Args:
        messages: 消息列表.

    Raises:
        ValidationError: 如果 messages 为空.
    """
    if not messages:
        raise ValidationError(
            field="messages",
            reason="messages list must not be empty",
        )


class AdapterRegistry:
    """协议适配器注册表, 将 Protocol_Type 标识符映射到 ProtocolAdapter 实例.

    支持注册, 解析 (含 OpenAI 回退) 和查询操作. 
    """

    def __init__(self) -> None:
        self._adapters: dict[str, ProtocolAdapter] = {}

    def register(self, protocol_type: str, adapter: ProtocolAdapter) -> None:
        """注册协议适配器.

        如果 protocol_type 已存在, 替换现有映射. 

        Args:
            protocol_type: 协议类型标识符 (如 "openai", "anthropic") .
            adapter: ProtocolAdapter 实例.
        """
        self._adapters[protocol_type] = adapter

    def resolve(self, protocol_type: str) -> ProtocolAdapter:
        """解析 protocol_type 到对应的适配器实例.

        如果 protocol_type 未注册, 回退到 "openai" 适配器并记录警告日志. 

        Args:
            protocol_type: 协议类型标识符.

        Returns:
            对应的 ProtocolAdapter 实例.
        """
        if protocol_type in self._adapters:
            return self._adapters[protocol_type]
        logger.warning(
            f"Unrecognized protocol_type '{protocol_type}', falling back to 'openai'",
        )
        return self._adapters["openai"]

    def has(self, protocol_type: str) -> bool:
        """检查指定协议类型是否已注册.

        Args:
            protocol_type: 协议类型标识符.

        Returns:
            如果已注册返回 True, 否则 False.
        """
        return protocol_type in self._adapters
