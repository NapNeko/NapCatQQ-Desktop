# -*- coding: utf-8 -*-
"""Anthropic Protocol Adapter.

实现 Anthropic Messages API 的原生协议适配器, 支持: 
- 系统消息提取到顶层 `system` 参数
- Content block 格式 (text, tool_use, tool_result) 
- SSE 事件解析 (message_start, content_block_start, content_block_delta,
  content_block_stop, message_delta, message_stop) 
- x-api-key + anthropic-version 认证头

Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9, 4.10, 4.11, 4.12
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from typing import AsyncIterator

from src.core.agent.api_key_pool import pick_api_key
from src.core.agent.protocol import (
    HttpRequestSpec,
    ProtocolAdapter,
    _validate_messages_not_empty,
)
from src.core.agent.provider import ModelConfig, Provider
from src.core.agent.session import Message
from src.core.agent.stream import (
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    TextDelta,
    ToolCallComplete,
    ToolCallDelta,
    ToolCallStart,
)
from src.core.agent.tool import ToolResult

logger = logging.getLogger(__name__)

# Anthropic stop_reason → internal StreamEnd reason mapping
_STOP_REASON_MAP: dict[str, str] = {
    "end_turn": "stop",
    "tool_use": "tool_calls",
    "max_tokens": "length",
}

# Default Anthropic API version
_DEFAULT_ANTHROPIC_VERSION = "2023-06-01"


@dataclass
class _PendingToolUseBlock:
    """跟踪正在累积的 tool_use content block.

    Attributes:
        tool_call_id: 工具调用 ID.
        function_name: 函数名.
        accumulated_json: 已累积的 JSON 参数片段.
    """

    tool_call_id: str
    function_name: str
    accumulated_json: str = ""


class AnthropicAdapter(ProtocolAdapter):
    """Anthropic Messages API 协议适配器.

    将内部消息格式转换为 Anthropic API 请求载荷, 并解析 Anthropic SSE
    流式响应为统一的 StreamEvent 序列. 
    """

    def build_request(
        self,
        messages: list[Message],
        tool_definitions: list[dict],
        model_config: ModelConfig,
        provider: Provider,
    ) -> HttpRequestSpec:
        """构建 Anthropic Messages API 请求.

        系统消息从 messages 中提取到顶层 `system` 参数. 

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
        _validate_messages_not_empty(messages)

        # Construct URL
        base_url = str(provider.api_base_url).rstrip("/")
        url = f"{base_url}/messages"

        # Build headers
        headers = self.build_headers(pick_api_key(provider.api_key_ref))

        # Extract system message and convert remaining messages
        system_content, converted_messages = self._extract_system_and_convert(messages)

        # Build request body
        body: dict = {
            "model": model_config.model_id,
            "messages": converted_messages,
            "max_tokens": model_config.max_tokens,
            "stream": True,
        }

        # Add system message to top-level if present
        if system_content is not None:
            body["system"] = system_content

        # Optional fields
        if tool_definitions:
            body["tools"] = self._convert_tool_definitions(tool_definitions)

        if model_config.temperature is not None:
            body["temperature"] = model_config.temperature
        if model_config.top_p is not None:
            body["top_p"] = model_config.top_p

        return HttpRequestSpec(
            method="POST",
            url=url,
            headers=headers,
            body=body,
        )

    async def parse_stream(
        self, response_lines: AsyncIterator[str]
    ) -> AsyncIterator[StreamEvent]:
        """解析 Anthropic SSE 流式响应为 StreamEvent 序列.

        处理的事件类型: 
        - message_start: 消息开始 (忽略) 
        - content_block_start: 内容块开始 (tool_use → ToolCallStart) 
        - content_block_delta: 内容块增量 (text_delta → TextDelta,
          input_json_delta → ToolCallDelta) 
        - content_block_stop: 内容块结束 (tool_use → ToolCallComplete) 
        - message_delta: 消息增量 (stop_reason → StreamEnd) 
        - message_stop: 消息结束 (忽略) 

        Args:
            response_lines: SSE 行的异步迭代器.

        Yields:
            StreamEvent 实例.
        """
        # Track pending tool_use blocks by content block index
        pending_tool_blocks: dict[int, _PendingToolUseBlock] = {}
        # Track current content block index and type
        current_block_index: int = -1
        current_block_type: str | None = None

        try:
            async for line in response_lines:
                line = line.rstrip("\r\n")

                # Skip empty lines and comments
                if not line or line.startswith(":"):
                    continue

                # Parse SSE event type
                if line.startswith("event:"):
                    # Store event type for next data line
                    current_event_type = line[6:].strip()
                    continue

                # Parse SSE data
                if not line.startswith("data:"):
                    continue

                data_str = line[5:].lstrip(" ")

                # Try to parse JSON
                try:
                    data = json.loads(data_str)
                except (json.JSONDecodeError, ValueError) as exc:
                    yield StreamErrorEvent(
                        status_code=None,
                        message=f"Malformed JSON in Anthropic SSE data: {exc}",
                    )
                    return

                # Check for error events
                if data.get("type") == "error":
                    error_info = data.get("error", {})
                    yield StreamErrorEvent(
                        status_code=None,
                        message=error_info.get("message", "Unknown Anthropic error"),
                    )
                    return

                # Route by event type
                event_type = data.get("type", "")

                if event_type == "message_start":
                    # Message start - nothing to emit
                    continue

                elif event_type == "content_block_start":
                    index = data.get("index", 0)
                    content_block = data.get("content_block", {})
                    block_type = content_block.get("type", "")
                    current_block_index = index
                    current_block_type = block_type

                    if block_type == "tool_use":
                        tool_id = content_block.get("id", "")
                        tool_name = content_block.get("name", "")
                        # Store pending block
                        pending_tool_blocks[index] = _PendingToolUseBlock(
                            tool_call_id=tool_id,
                            function_name=tool_name,
                        )
                        # Emit ToolCallStart
                        yield ToolCallStart(
                            tool_call_id=tool_id,
                            function_name=tool_name,
                        )

                elif event_type == "content_block_delta":
                    index = data.get("index", 0)
                    delta = data.get("delta", {})
                    delta_type = delta.get("type", "")

                    if delta_type == "text_delta":
                        text = delta.get("text", "")
                        if text:
                            yield TextDelta(text=text)

                    elif delta_type == "input_json_delta":
                        partial_json = delta.get("partial_json", "")
                        if index in pending_tool_blocks:
                            pending = pending_tool_blocks[index]
                            pending.accumulated_json += partial_json
                            # Emit ToolCallDelta
                            yield ToolCallDelta(
                                tool_call_id=pending.tool_call_id,
                                arguments_delta=partial_json,
                            )

                elif event_type == "content_block_stop":
                    index = data.get("index", 0)
                    # If this was a tool_use block, emit ToolCallComplete
                    if index in pending_tool_blocks:
                        pending = pending_tool_blocks.pop(index)
                        yield ToolCallComplete(
                            tool_call_id=pending.tool_call_id,
                            function_name=pending.function_name,
                            arguments=pending.accumulated_json,
                        )

                elif event_type == "message_delta":
                    delta = data.get("delta", {})
                    stop_reason = delta.get("stop_reason")
                    if stop_reason:
                        reason = _STOP_REASON_MAP.get(stop_reason, "stop")
                        yield StreamEnd(reason=reason)

                elif event_type == "message_stop":
                    # Message complete - nothing additional to emit
                    continue

        except Exception as exc:
            yield StreamErrorEvent(
                status_code=None,
                message=f"Anthropic stream parsing error: {exc}",
            )

    def build_headers(self, api_key: str) -> dict[str, str]:
        """构建 Anthropic API 认证请求头.

        Args:
            api_key: API 密钥字符串.

        Returns:
            包含 x-api-key, anthropic-version, Content-Type 的请求头字典.
        """
        return {
            "x-api-key": api_key,
            "anthropic-version": _DEFAULT_ANTHROPIC_VERSION,
            "Content-Type": "application/json",
        }

    def build_tool_result_payload(
        self, tool_call_id: str, tool_result: ToolResult
    ) -> list[dict]:
        """构建 Anthropic 格式的工具结果消息载荷.

        Anthropic 使用 user 角色消息中的 tool_result content block. 

        Args:
            tool_call_id: 工具调用 ID.
            tool_result: 工具执行结果.

        Returns:
            包含 user 角色消息 (含 tool_result block) 的列表.
        """
        content_block: dict = {
            "type": "tool_result",
            "tool_use_id": tool_call_id,
            "content": tool_result.output,
        }

        if tool_result.is_error:
            content_block["is_error"] = True

        return [
            {
                "role": "user",
                "content": [content_block],
            }
        ]

    def _extract_system_and_convert(
        self, messages: list[Message]
    ) -> tuple[str | None, list[dict]]:
        """提取系统消息并转换剩余消息为 Anthropic 格式.

        系统消息 (role="system" 在内部表示中不存在于 Message Literal, 
        但可能通过 content 前缀或特殊标记传递) 被提取到顶层. 
        实际上, 内部 Message 的 role 只有 user/assistant/tool, 
        系统消息通常作为第一条 user 消息的特殊处理. 

        根据设计文档, 如果 messages 中包含 system-role 消息, 
        将其提取到顶层 system 参数. 由于 Message.role 是 Literal["user", "assistant", "tool"], 
        我们检查是否有消息的 content 以特殊前缀标记为系统消息, 
        或者通过外部传入的方式处理. 

        实际实现中, 系统消息可能通过 Message 的扩展字段传递. 
        这里我们检查第一条消息是否标记为系统消息 (通过检查 role 字段, 
        虽然 Literal 约束了类型, 但运行时可能有 "system" 值传入) . 

        Args:
            messages: 内部消息列表.

        Returns:
            (system_content, converted_messages) 元组.
        """
        system_content: str | None = None
        converted: list[dict] = []

        for msg in messages:
            # Check for system message (runtime role value may be "system"
            # even though type annotation says Literal["user", "assistant", "tool"])
            if getattr(msg, "role", None) == "system":
                system_content = msg.content
                continue

            converted_msg = self._convert_single_message(msg)
            if converted_msg is not None:
                converted.append(converted_msg)

        return system_content, converted

    def _convert_single_message(self, msg: Message) -> dict | None:
        """将单个 Message 转换为 Anthropic API 消息格式.

        转换规则: 
        - user → {"role": "user", "content": [{"type": "text", "text": content}]}
        - assistant (无 tool_calls) → {"role": "assistant", "content": [{"type": "text", "text": content}]}
        - assistant (有 tool_calls) → {"role": "assistant", "content": [text block + tool_use blocks]}
        - tool → {"role": "user", "content": [{"type": "tool_result", "tool_use_id": id, "content": content}]}

        Args:
            msg: 内部消息.

        Returns:
            Anthropic 格式的消息字典, 或 None.
        """
        if msg.role == "user":
            return {
                "role": "user",
                "content": [{"type": "text", "text": msg.content}],
            }

        elif msg.role == "assistant":
            content_blocks: list[dict] = []

            # Add text content if present
            if msg.content:
                content_blocks.append({"type": "text", "text": msg.content})

            # Add tool_use blocks if present
            if msg.tool_calls:
                for tc in msg.tool_calls:
                    tool_use_block: dict = {
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.function_name,
                        "input": self._parse_arguments(tc.arguments),
                    }
                    content_blocks.append(tool_use_block)

            # If no content blocks, add empty text
            if not content_blocks:
                content_blocks.append({"type": "text", "text": ""})

            return {
                "role": "assistant",
                "content": content_blocks,
            }

        elif msg.role == "tool":
            return {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id or "",
                        "content": msg.content,
                    }
                ],
            }

        return None

    def _convert_tool_definitions(self, tool_definitions: list[dict]) -> list[dict]:
        """将 OpenAI 格式的工具定义转换为 Anthropic 格式.

        OpenAI 格式:
        {
            "type": "function",
            "function": {
                "name": "...",
                "description": "...",
                "parameters": {...}
            }
        }

        Anthropic 格式:
        {
            "name": "...",
            "description": "...",
            "input_schema": {...}
        }

        Args:
            tool_definitions: OpenAI 格式的工具定义列表.

        Returns:
            Anthropic 格式的工具定义列表.
        """
        anthropic_tools: list[dict] = []

        for tool_def in tool_definitions:
            function_info = tool_def.get("function", {})
            if not function_info:
                # If tool_def itself has name/description/parameters (flat format)
                function_info = tool_def

            anthropic_tool: dict = {
                "name": function_info.get("name", ""),
                "description": function_info.get("description", ""),
                "input_schema": function_info.get("parameters", {}),
            }
            anthropic_tools.append(anthropic_tool)

        return anthropic_tools

    @staticmethod
    def _parse_arguments(arguments: str) -> dict:
        """解析 JSON 参数字符串为字典.

        Args:
            arguments: JSON 编码的参数字符串.

        Returns:
            解析后的字典, 解析失败时返回空字典.
        """
        if not arguments:
            return {}
        try:
            result = json.loads(arguments)
            if isinstance(result, dict):
                return result
            return {}
        except (json.JSONDecodeError, ValueError):
            return {}
